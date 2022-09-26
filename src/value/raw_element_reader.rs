use crate::result::{decoding_error, illegal_operation, illegal_operation_raw};
use crate::text::parent_container::ParentContainer;
use crate::value::owned::Element;
use crate::value::{IonElement, IonSequence, IonStruct, IonSymbolToken};
use crate::{
    Decimal, Integer, IonError, IonReader, IonResult, IonType, RawStreamItem, Symbol, Timestamp,
};
use std::fmt::Display;
use std::mem;

const INITIAL_PARENTS_CAPACITY: usize = 16;

struct ElementIteratorItem<S: IonSymbolToken> {
    // If the Iterator item is a field then the field_name is set as the field name of that field
    // Otherwise this is set as None
    field_name: Option<S>,
    value: Element,
}

impl<S: IonSymbolToken> ElementIteratorItem<S> {
    fn field_name(&self) -> &Option<S> {
        &self.field_name
    }

    fn value(&self) -> &Element {
        &self.value
    }
}

pub struct RawElementReader {
    // Represents the element that will be read using this reader
    element: Element,
    current_iter: Box<dyn Iterator<Item = (Option<Symbol>, Element)>>,
    iter_stack: Vec<Box<dyn Iterator<Item = (Option<Symbol>, Element)>>>,
    // If the reader is not positioned over a value inside a struct, this is None.
    current_field_name: Option<Symbol>,
    // If the reader has not yet begun reading at the current level or is positioned over an IVM,
    // this is None.
    current_value: Option<Element>,
    is_eof: bool,
    parents: Vec<ParentContainer>,
}

impl RawElementReader {
    pub fn new(input: Element) -> RawElementReader {
        RawElementReader {
            element: input,
            current_iter: Box::new(std::iter::empty()),
            iter_stack: vec![],
            current_field_name: None,
            current_value: None,
            is_eof: false,
            parents: Vec::with_capacity(INITIAL_PARENTS_CAPACITY),
        }
    }

    fn load_next_value(&mut self) -> IonResult<()> {
        // If the reader's current value is the beginning of a container and the user calls `next()`,
        // we need to skip the entire container. We can do this by stepping into and then out of
        // that container; `step_out()` has logic that will exhaust the remaining values.
        let need_to_skip_container = !self.is_null()
            && self
                .current_value
                .as_ref()
                .map(|v| v.ion_type().is_container())
                .unwrap_or(false);

        if need_to_skip_container {
            self.step_in()?;
            self.step_out()?;
        }

        // Unset variables holding onto information about the previous position.
        self.current_value = None;
        self.current_field_name = None;

        if self.parents.is_empty() {
            // If the reader has already found EOF (the end of the top level), there's no need to
            // try to read more data. Return Ok(None).
            if self.is_eof {
                self.current_value = None;
                return Ok(());
            }

            self.current_value = Some(self.element.to_owned());
            // As we already loaded the single element provided to the reader, we have reached eof
            self.is_eof = true;
            return Ok(());
        }

        // If the parent is not empty that means we are inside a container
        // get the next value of the container using the iterator
        let next_item = self.current_iter.next();
        if next_item == None {
            // If there are no next values left within the iterator
            // then return None
            self.current_value = None;
            return Ok(());
        }
        // Otherwise if there is a next value available then set current value accordingly
        let (field_name, value) = next_item.unwrap();
        self.current_value = Some(value);
        // Field name will either be `None` for container types SExpression, List
        // But for struct it will contain the field name `Symbol`
        self.current_field_name = field_name;

        Ok(())
    }

    /// Constructs an [IonError::IllegalOperation] which explains that the reader was asked to
    /// perform an action that is only allowed when it is positioned over the item type described
    /// by the parameter `expected`.
    fn expected<I: Display>(&self, expected: I) -> IonError {
        illegal_operation_raw(format!(
            "type mismatch: expected a(n) {} but positioned over a(n) {}",
            expected,
            self.current()
        ))
    }

    fn map_iterator(e: Element) -> Box<dyn Iterator<Item = (Option<Symbol>, Element)>> {
        Box::new(
            e.as_sequence()
                .unwrap()
                .iter()
                .map(|e| (None, e.to_owned()))
                .collect::<Vec<(Option<Symbol>, Element)>>()
                .into_iter(),
        )
    }
}

impl IonReader for RawElementReader {
    type Item = RawStreamItem;
    type Symbol = Symbol;

    fn next(&mut self) -> IonResult<RawStreamItem> {
        // Parse the next value from the stream, storing it in `self.current_value`.
        self.load_next_value()?;

        // If we're positioned on a value, return its IonType and whether it's null.
        if let Some(value) = self.current_value.as_ref() {
            Ok(RawStreamItem::nullable_value(
                value.ion_type(),
                value.is_null(),
            ))
        } else {
            Ok(RawStreamItem::Nothing)
        }
    }

    fn current(&self) -> RawStreamItem {
        if let Some(ref value) = self.current_value {
            RawStreamItem::nullable_value(value.ion_type(), value.is_null())
        } else {
            RawStreamItem::Nothing
        }
    }

    fn ion_type(&self) -> Option<IonType> {
        if let Some(ref value) = self.current_value {
            return Some(value.ion_type());
        }
        None
    }

    fn is_null(&self) -> bool {
        if let Some(ref value) = self.current_value {
            return value.is_null();
        }
        false
    }

    fn annotations<'a>(&'a self) -> Box<dyn Iterator<Item = IonResult<Self::Symbol>> + 'a> {
        let iterator = self
            .current_value
            .as_ref()
            .map(|value| value.annotations())
            .unwrap_or_else(|| Box::new(std::iter::empty()))
            .cloned()
            // The annotations are already in memory and are already resolved to text, so
            // this step cannot fail. Map each token to Ok(token).
            .map(Ok);
        Box::new(iterator)
    }

    fn has_annotations(&self) -> bool {
        self.current_value
            .as_ref()
            .map(|value| value.annotations().peekable().peek() != None)
            .unwrap_or(false)
    }

    fn number_of_annotations(&self) -> usize {
        self.current_value
            .as_ref()
            .map(|value| value.annotations().count())
            .unwrap_or(0)
    }

    fn field_name(&self) -> IonResult<Self::Symbol> {
        match self.current_field_name.as_ref() {
            Some(name) => Ok(name.clone()),
            None => illegal_operation(
                "field_name() can only be called when the reader is positioned inside a struct",
            ),
        }
    }

    // TODO: See if the match statements for read_*() below could be simplified

    fn read_null(&mut self) -> IonResult<IonType> {
        match self.current_value.as_ref() {
            Some(element) if element.is_null() => Ok(element.ion_type()),
            _ => Err(self.expected("null value")),
        }
    }

    fn read_bool(&mut self) -> IonResult<bool> {
        match self.current_value.as_ref() {
            Some(element) if element.as_bool().is_some() => Ok(element.as_bool().unwrap()),
            _ => Err(self.expected("bool value")),
        }
    }

    fn read_integer(&mut self) -> IonResult<Integer> {
        match self.current_value.as_ref() {
            Some(element) if element.as_integer().is_some() => {
                Ok(element.as_integer().unwrap().clone())
            }
            _ => Err(self.expected("int value")),
        }
    }

    fn read_i64(&mut self) -> IonResult<i64> {
        match self.current_value.as_ref() {
            Some(element) if element.as_integer().is_some() => {
                match element.as_integer().unwrap() {
                    Integer::I64(value) => Ok(*value),
                    Integer::BigInt(value) => {
                        decoding_error(format!("Integer {} is too large to fit in an i64.", value))
                    }
                }
            }
            _ => Err(self.expected("int value")),
        }
    }

    fn read_f32(&mut self) -> IonResult<f32> {
        match self.current_value.as_ref() {
            Some(element) if element.as_f64().is_some() => Ok(element.as_f64().unwrap() as f32),
            _ => Err(self.expected("float value")),
        }
    }

    fn read_f64(&mut self) -> IonResult<f64> {
        match self.current_value.as_ref() {
            Some(element) if element.as_f64().is_some() => Ok(element.as_f64().unwrap()),
            _ => Err(self.expected("float value")),
        }
    }

    fn read_decimal(&mut self) -> IonResult<Decimal> {
        match self.current_value.as_ref() {
            Some(element) if element.as_decimal().is_some() => {
                Ok(element.as_decimal().unwrap().clone())
            }
            _ => Err(self.expected("decimal value")),
        }
    }

    fn read_string(&mut self) -> IonResult<String> {
        self.map_string(|s| s.to_owned())
    }

    fn map_string<F, U>(&mut self, f: F) -> IonResult<U>
    where
        Self: Sized,
        F: FnOnce(&str) -> U,
    {
        match self.current_value.as_ref() {
            Some(element) if element.as_str().is_some() => Ok(f(element.as_str().unwrap())),
            _ => Err(self.expected("string value")),
        }
    }

    fn map_string_bytes<F, U>(&mut self, f: F) -> IonResult<U>
    where
        Self: Sized,
        F: FnOnce(&[u8]) -> U,
    {
        // In the binary reader, this method can bypass utf-8 validation and return a view of the
        // raw bytes in the input buffer. In the text reader, this optimization isn't available;
        // some of the input bytes may be encoded as text unicode escapes and require processing
        // to turn into &[u8].
        self.map_string(|s| f(s.as_bytes()))
    }

    fn read_symbol(&mut self) -> IonResult<Self::Symbol> {
        match self.current_value.as_ref() {
            Some(element) if element.as_sym().is_some() => Ok(element.as_sym().unwrap().clone()),
            _ => Err(self.expected("symbol value")),
        }
    }

    fn read_blob(&mut self) -> IonResult<Vec<u8>> {
        self.map_blob(|b| Vec::from(b))
    }

    fn map_blob<F, U>(&mut self, f: F) -> IonResult<U>
    where
        Self: Sized,
        F: FnOnce(&[u8]) -> U,
    {
        match self.current_value.as_ref() {
            Some(element) if element.as_bytes().is_some() => Ok(f(element.as_bytes().unwrap())),
            _ => Err(self.expected("blob value")),
        }
    }

    fn read_clob(&mut self) -> IonResult<Vec<u8>> {
        self.map_clob(|c| Vec::from(c))
    }

    fn map_clob<F, U>(&mut self, f: F) -> IonResult<U>
    where
        Self: Sized,
        F: FnOnce(&[u8]) -> U,
    {
        match self.current_value.as_ref() {
            Some(element) if element.as_bytes().is_some() => Ok(f(element.as_bytes().unwrap())),
            _ => Err(self.expected("clob value")),
        }
    }

    fn read_timestamp(&mut self) -> IonResult<Timestamp> {
        match self.current_value.as_ref() {
            Some(element) if element.as_timestamp().is_some() => {
                Ok(element.as_timestamp().unwrap().clone())
            }
            _ => Err(self.expected("timestamp value")),
        }
    }

    fn step_in(&mut self) -> IonResult<()> {
        match &self.current_value {
            Some(value) if value.ion_type().is_container() => {
                self.parents.push(ParentContainer::new(value.ion_type()));
                let new_iter = mem::replace(&mut self.current_iter, Box::new(std::iter::empty()));
                self.iter_stack.push(new_iter);
                self.current_iter = match value.ion_type() {
                    IonType::List | IonType::SExpression => Box::new(
                        value
                            .as_sequence()
                            .unwrap()
                            .iter()
                            .map(|e| (None, e.to_owned()))
                            .collect::<Vec<(Option<Symbol>, Element)>>()
                            .into_iter(),
                    ),
                    IonType::Struct => Box::new(
                        value
                            .as_struct()
                            .unwrap()
                            .iter()
                            .map(|(s, e)| (Some(s.to_owned()), e.to_owned()))
                            .collect::<Vec<(Option<Symbol>, Element)>>()
                            .into_iter(),
                    ),
                    _ => unreachable!("Can not step into a scalar type"),
                };
                self.current_value = None;
                Ok(())
            }
            Some(value) => {
                illegal_operation(format!("Cannot step_in() to a {:?}", value.ion_type()))
            }
            None => illegal_operation(format!(
                "{} {}",
                "Cannot `step_in`: the reader is not positioned on a value.",
                "Try calling `next()` to advance first."
            )),
        }
    }

    fn step_out(&mut self) -> IonResult<()> {
        if self.parents.is_empty() {
            return illegal_operation(
                "Cannot call `step_out()` when the reader is at the top level.",
            );
        }

        // The container we're stepping out of.
        let parent = self.parents.last().unwrap();

        // If we're not at the end of the current container, advance the cursor until we are.
        // Unlike the binary reader, which can skip-scan, the text reader must visit every value
        // between its current position and the end of the container.
        if !parent.is_exhausted() {
            while let RawStreamItem::Value(_) | RawStreamItem::Null(_) = self.next()? {}
        }

        // Remove the parent container from the stack and clear the current value.
        let _ = self.parents.pop();

        // Remove the iterator related to the parent container from stack and set it as current iterator
        match self.iter_stack.pop() {
            None => {}
            Some(iter) => {
                self.current_iter = iter;
            }
        }
        self.current_value = None;

        if self.parents.is_empty() {
            // We're at the top level; nothing left to do.
            return Ok(());
        }

        Ok(())
    }

    fn parent_type(&self) -> Option<IonType> {
        self.parents.last().map(|parent| parent.ion_type())
    }

    fn depth(&self) -> usize {
        self.parents.len()
    }

    fn ion_version(&self) -> (u8, u8) {
        todo!()
    }
}

fn next_type(reader: &mut RawElementReader, ion_type: IonType, is_null: bool) {
    assert_eq!(
        reader.next().unwrap(),
        RawStreamItem::nullable_value(ion_type, is_null)
    );
}

#[cfg(test)]
mod reader_tests {
    use rstest::*;

    use super::*;
    use crate::result::IonResult;
    use crate::stream_reader::IonReader;
    use crate::types::decimal::Decimal;
    use crate::types::timestamp::Timestamp;
    use crate::value::owned::text_token;
    use crate::value::reader::{element_reader, ElementReader};
    use crate::IonType;

    fn load_element(text: &str) -> Element {
        element_reader()
            .read_one(text.as_bytes())
            .expect("parsing failed unexpectedly")
    }

    fn next_type(reader: &mut RawElementReader, ion_type: IonType, is_null: bool) {
        assert_eq!(
            reader.next().unwrap(),
            RawStreamItem::nullable_value(ion_type, is_null)
        );
    }

    #[test]
    fn test_skipping_containers() -> IonResult<()> {
        let ion_data = load_element(
            r#"
            [1, 2, 3]
        "#,
        );
        let reader = &mut RawElementReader::new(ion_data);

        next_type(reader, IonType::List, false);
        reader.step_in()?;
        next_type(reader, IonType::Integer, false);
        assert_eq!(reader.read_i64()?, 1);
        reader.step_out()?;
        // This should skip 2, 3 and reach end of the element
        // Asking for next here should result in `Nothing`
        assert_eq!(reader.next()?, RawStreamItem::Nothing);
        Ok(())
    }

    #[test]
    fn test_read_nested_containers() -> IonResult<()> {
        let ion_data = load_element(
            r#"
            {
                foo: [
                    1,
                    [2, 3],
                    4
                ],
                bar: {
                    a: 5,
                    b: (true true true)
                }
            }
        "#,
        );
        let reader = &mut RawElementReader::new(ion_data);
        next_type(reader, IonType::Struct, false);
        reader.step_in()?;
        next_type(reader, IonType::List, false);
        reader.step_in()?;
        next_type(reader, IonType::Integer, false);
        next_type(reader, IonType::List, false);
        reader.step_in()?;
        next_type(reader, IonType::Integer, false);
        // The reader is now at the '2' nested inside of 'foo'
        reader.step_out()?;
        reader.step_out()?;
        next_type(reader, IonType::Struct, false);
        reader.step_in()?;
        next_type(reader, IonType::Integer, false);
        next_type(reader, IonType::SExpression, false);
        reader.step_in()?;
        next_type(reader, IonType::Boolean, false);
        next_type(reader, IonType::Boolean, false);
        // The reader is now at the second 'true' in the s-expression nested in 'bar'/'b'
        reader.step_out()?;
        reader.step_out()?;
        reader.step_out()?;
        Ok(())
    }

    #[test]
    fn test_read_container_with_mixed_scalars_and_containers() -> IonResult<()> {
        let ion_data = load_element(
            r#"
            {
                foo: 4,
                bar: {
                    a: 5,
                    b: (true true true)
                }
            }
        "#,
        );

        let reader = &mut RawElementReader::new(ion_data);
        next_type(reader, IonType::Struct, false);
        reader.step_in()?;
        next_type(reader, IonType::Integer, false);
        assert_eq!(reader.field_name()?, text_token("foo"));
        next_type(reader, IonType::Struct, false);
        assert_eq!(reader.field_name()?, text_token("bar"));
        reader.step_in()?;
        next_type(reader, IonType::Integer, false);
        assert_eq!(reader.read_i64()?, 5);
        reader.step_out()?;
        assert_eq!(reader.next()?, RawStreamItem::Nothing);
        reader.step_out()?;
        Ok(())
    }

    #[rstest]
    #[case(" true ", true)]
    #[case(" false ", false)]
    #[case(" 738 ", 738)]
    #[case(" 2.5e0 ", 2.5)]
    #[case(" 2.5 ", Decimal::new(25, -1))]
    #[case(" 2007-07-12T ", Timestamp::with_ymd(2007, 7, 12).build().unwrap())]
    #[case(" foo ", text_token("foo"))]
    #[case(" \"hi!\" ", "hi!".to_owned())]
    fn test_read_single_top_level_values<E: Into<Element>>(
        #[case] text: &str,
        #[case] expected_value: E,
    ) {
        let reader = &mut RawElementReader::new(load_element(text));
        let expected_element = expected_value.into();
        next_type(
            reader,
            expected_element.ion_type(),
            expected_element.is_null(),
        );
        // TODO: Redo (or remove?) this test. There's not an API that exposes the
        //       AnnotatedTextValue any more. We're directly accessing `current_value` as a hack.
        let actual_element = reader.current_value.clone();
        assert_eq!(actual_element.unwrap(), expected_element);
    }

    #[test]
    fn structs_trailing_comma() -> IonResult<()> {
        let pretty_ion = load_element(
            r#"
            // Structs with last field with/without trailing comma
            (
                {a:1, b:2,}     // with trailing comma
                {a:1, b:2 }     // without trailing comma
            )
        "#,
        );
        let mut reader = RawElementReader::new(pretty_ion);
        assert_eq!(reader.next()?, RawStreamItem::Value(IonType::SExpression));
        reader.step_in()?;
        assert_eq!(reader.next()?, RawStreamItem::Value(IonType::Struct));

        reader.step_in()?;
        assert_eq!(reader.next()?, RawStreamItem::Value(IonType::Integer));
        assert_eq!(reader.field_name()?, Symbol::owned("a".to_string()));
        assert_eq!(reader.read_i64()?, 1);
        assert_eq!(reader.next()?, RawStreamItem::Value(IonType::Integer));
        assert_eq!(reader.field_name()?, Symbol::owned("b".to_string()));
        assert_eq!(reader.read_i64()?, 2);
        reader.step_out()?;

        assert_eq!(reader.next()?, RawStreamItem::Value(IonType::Struct));
        reader.step_out()?;
        Ok(())
    }
}
