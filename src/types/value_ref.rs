use crate::result::{decoding_error, decoding_error_raw};

use crate::value_reader::{SequenceRef, StructRef};
use crate::{
    Decimal, Int, IonResult, IonType, RawIonReader, RawSymbolTokenRef, Symbol, SystemReader,
    Timestamp, ValueReader,
};

/// As RawValueRef represents a reference to a value in data stream, the container variants
/// simply indicate their Ion type. To access their nested data, the reader would need to step in.
#[derive(Debug, PartialEq)]
pub enum RawValueRef<'a> {
    Null(IonType),
    Bool(bool),
    Int(Int),
    Float(f64),
    Decimal(Decimal),
    Timestamp(Timestamp),
    String(&'a str),
    Symbol(RawSymbolTokenRef<'a>),
    Blob(&'a [u8]),
    Clob(&'a [u8]),
    // As ValueRef represents a reference to a value in the streaming APIs, the container variants
    // simply indicate their Ion type. To access their nested data, the reader would need to step in.
    SExp,
    List,
    Struct,
}

impl<'a> RawValueRef<'a> {
    pub fn expect_null(self) -> IonResult<IonType> {
        if let RawValueRef::Null(ion_type) = self {
            Ok(ion_type)
        } else {
            decoding_error("expected a null")
        }
    }

    pub fn expect_bool(self) -> IonResult<bool> {
        if let RawValueRef::Bool(b) = self {
            Ok(b)
        } else {
            decoding_error("expected a bool")
        }
    }

    pub fn expect_int(self) -> IonResult<Int> {
        if let RawValueRef::Int(i) = self {
            Ok(i)
        } else {
            decoding_error("expected an int")
        }
    }

    pub fn expect_float(self) -> IonResult<f64> {
        if let RawValueRef::Float(f) = self {
            Ok(f)
        } else {
            decoding_error("expected a float")
        }
    }

    pub fn expect_decimal(self) -> IonResult<Decimal> {
        if let RawValueRef::Decimal(d) = self {
            Ok(d)
        } else {
            decoding_error("expected a decimal")
        }
    }

    pub fn expect_timestamp(self) -> IonResult<Timestamp> {
        if let RawValueRef::Timestamp(t) = self {
            Ok(t)
        } else {
            decoding_error("expected a timestamp")
        }
    }

    pub fn expect_string(self) -> IonResult<&'a str> {
        if let RawValueRef::String(s) = self {
            Ok(s)
        } else {
            decoding_error("expected a string")
        }
    }

    pub fn expect_symbol(self) -> IonResult<RawSymbolTokenRef<'a>> {
        if let RawValueRef::Symbol(s) = self {
            Ok(s.clone())
        } else {
            decoding_error("expected a symbol")
        }
    }

    pub fn expect_blob(self) -> IonResult<&'a [u8]> {
        if let RawValueRef::Blob(b) = self {
            Ok(b)
        } else {
            decoding_error("expected a blob")
        }
    }

    pub fn expect_clob(self) -> IonResult<&'a [u8]> {
        if let RawValueRef::Clob(c) = self {
            Ok(c)
        } else {
            decoding_error("expected a clob")
        }
    }

    pub fn expect_list(self) -> IonResult<()> {
        if let RawValueRef::List = self {
            Ok(())
        } else {
            decoding_error("expected a list")
        }
    }

    pub fn expect_sexp(self) -> IonResult<()> {
        if let RawValueRef::SExp = self {
            Ok(())
        } else {
            decoding_error("expected a sexp")
        }
    }

    pub fn expect_struct(self) -> IonResult<()> {
        if let RawValueRef::Struct = self {
            Ok(())
        } else {
            decoding_error("expected a struct")
        }
    }
}

pub trait ReadRawValueRef {
    fn read_value(&self) -> IonResult<RawValueRef>;

    fn read_null(&self) -> IonResult<IonType> {
        self.read_value()?.expect_null()
    }

    fn read_bool(&self) -> IonResult<bool> {
        self.read_value()?.expect_bool()
    }

    fn read_int(&self) -> IonResult<Int> {
        self.read_value()?.expect_int()
    }

    fn read_float(&self) -> IonResult<f64> {
        self.read_value()?.expect_float()
    }

    fn read_decimal(&self) -> IonResult<Decimal> {
        self.read_value()?.expect_decimal()
    }

    fn read_timestamp(&self) -> IonResult<Timestamp> {
        self.read_value()?.expect_timestamp()
    }

    fn read_string(&self) -> IonResult<&str> {
        self.read_value()?.expect_string()
    }

    fn read_symbol(&self) -> IonResult<RawSymbolTokenRef> {
        self.read_value()?.expect_symbol()
    }

    fn read_clob(&self) -> IonResult<&[u8]> {
        self.read_value()?.expect_clob()
    }

    fn read_blob(&self) -> IonResult<&[u8]> {
        self.read_value()?.expect_blob()
    }

    fn as_list(&self) -> IonResult<()> {
        self.read_value()?.expect_list()
    }

    fn as_sexp(&self) -> IonResult<()> {
        self.read_value()?.expect_sexp()
    }

    fn as_struct(&self) -> IonResult<()> {
        self.read_value()?.expect_struct()
    }
}

impl<R: RawIonReader> ReadRawValueRef for R {
    fn read_value(&self) -> IonResult<RawValueRef> {
        <Self as RawIonReader>::read_value(self)
    }
}

/// A reference to a value in the input stream. Where possible, text and lob values will be a
/// zero-copy slice of input data.
///
/// Scalar values can be directly accessed by matching on their corresponding variants.
///
/// ```
/// use ion_rs::{IonReader, IonResult, ReaderBuilder, ValueRef, ReadValueRef, ReadRawValueRef, StreamItem};
///# fn main() -> IonResult<()> {
///
/// use ion_rs::{};
/// use ion_rs::types::integer::IntAccess;
///
/// let mut reader = ReaderBuilder::default().build("1 2 3")?;
/// let mut sum = 0;
///
/// while let StreamItem::Value(mut v) = reader.next()? {
///     if let ValueRef::Int(i) = v.read()? {
///         sum += i.as_i64().unwrap();
///     }
/// }
/// assert_eq!(sum, 6);
///
/// // Again, but using a typed 'read_int()' method to early return on a type mismatch
/// let mut reader = ReaderBuilder::default().build("1 2 3")?;
///
/// sum = 0;
/// while let StreamItem::Value(mut v) = reader.next()? {
///     sum += v.read_int()?.as_i64().unwrap();
/// }
///# Ok(())
///# }
/// ```
///
/// Container variants return a handle to the container value, which you can use to traverse the
/// contents of the container.
///
/// **Traversing a list**
/// ```
/// use ion_rs::{IonReader, IonResult, ReaderBuilder, ValueRef, ReadValueRef, ReadRawValueRef, StreamItem};
///# fn main() -> IonResult<()> {
///
/// use ion_rs::{};
/// use ion_rs::types::integer::IntAccess;
///
/// let mut reader = ReaderBuilder::default().build("[1, 2, 3]")?;
///
/// if let StreamItem::Value(mut value) = reader.next()? {
///   let mut list_reader = value.as_list()?.reader()?;
///   let mut sum = 0;
///   while let Some(mut child_value) = list_reader.next_element()? {
///     sum += child_value.read_i64()?;
///   }
///   assert_eq!(sum, 6);
/// }
///# Ok(())
///# }
/// ```
///
/// **Traversing a struct**
/// ```
/// use ion_rs::{IonReader, IonResult, ReaderBuilder, ValueRef, ReadValueRef, ReadRawValueRef, StreamItem};
///# fn main() -> IonResult<()> {
///
/// use ion_rs::{};
/// use ion_rs::types::integer::IntAccess;
///
/// let mut reader = ReaderBuilder::default().build("{a: 1, b: 2, c:3}")?;
///
/// if let StreamItem::Value(mut value) = reader.next()? {
///   let mut struct_reader = value.as_struct()?.reader()?;
///   let mut sum = 0;
///   while let Some(mut field) = struct_reader.next_field()? {
///     if field.read_name()? != "b" {
///       sum += field.value().read_i64()?;
///     }
///   }
///   assert_eq!(sum, 4);
/// }
///# Ok(())
///# }
/// ```
#[derive(Debug)]
pub enum ValueRef<'a, R: RawIonReader + 'a> {
    Null(IonType),
    Bool(bool),
    Int(Int),
    Float(f64),
    Decimal(Decimal),
    Timestamp(Timestamp),
    String(&'a str),
    Symbol(Symbol),
    Blob(&'a [u8]),
    Clob(&'a [u8]),
    // As ValueRef represents a reference to a value in the streaming APIs, the container variants
    // simply indicate their Ion type. To access their nested data, the reader would need to step in.
    SExp(SequenceRef<'a, R>),
    List(SequenceRef<'a, R>),
    Struct(StructRef<'a, R>),
}

impl<'a, R: RawIonReader> ValueRef<'a, R> {
    pub fn expect_null(self) -> IonResult<IonType> {
        if let ValueRef::Null(ion_type) = self {
            Ok(ion_type)
        } else {
            decoding_error("expected a null")
        }
    }

    pub fn expect_bool(self) -> IonResult<bool> {
        if let ValueRef::Bool(b) = self {
            Ok(b)
        } else {
            decoding_error("expected a bool")
        }
    }

    pub fn expect_int(self) -> IonResult<Int> {
        if let ValueRef::Int(i) = self {
            Ok(i)
        } else {
            decoding_error("expected an int")
        }
    }

    pub fn expect_float(self) -> IonResult<f64> {
        if let ValueRef::Float(f) = self {
            Ok(f)
        } else {
            decoding_error("expected a float")
        }
    }

    pub fn expect_decimal(self) -> IonResult<Decimal> {
        if let ValueRef::Decimal(d) = self {
            Ok(d)
        } else {
            decoding_error("expected a decimal")
        }
    }

    pub fn expect_timestamp(self) -> IonResult<Timestamp> {
        if let ValueRef::Timestamp(t) = self {
            Ok(t)
        } else {
            decoding_error("expected a timestamp")
        }
    }

    pub fn expect_string(self) -> IonResult<&'a str> {
        if let ValueRef::String(s) = self {
            Ok(s)
        } else {
            decoding_error("expected a string")
        }
    }

    pub fn expect_symbol(self) -> IonResult<Symbol> {
        if let ValueRef::Symbol(s) = self {
            Ok(s)
        } else {
            decoding_error("expected a symbol")
        }
    }

    pub fn expect_blob(self) -> IonResult<&'a [u8]> {
        if let ValueRef::Blob(b) = self {
            Ok(b)
        } else {
            decoding_error("expected a blob")
        }
    }

    pub fn expect_clob(self) -> IonResult<&'a [u8]> {
        if let ValueRef::Clob(c) = self {
            Ok(c)
        } else {
            decoding_error("expected a clob")
        }
    }

    pub fn expect_list(self) -> IonResult<SequenceRef<'a, R>> {
        if let ValueRef::List(s) = self {
            Ok(s)
        } else {
            decoding_error("expected a list")
        }
    }

    pub fn expect_sexp(self) -> IonResult<SequenceRef<'a, R>> {
        if let ValueRef::SExp(s) = self {
            Ok(s)
        } else {
            decoding_error("expected a sexp")
        }
    }

    pub fn expect_struct(self) -> IonResult<StructRef<'a, R>> {
        if let ValueRef::Struct(s) = self {
            Ok(s)
        } else {
            decoding_error("expected a struct")
        }
    }
}

// Note: the methods in this trait take a &mut because the resolved ValueRef can return a handle to
// continue reading a nested container. This requires advancing the reader and therefore mutability.
pub trait ReadValueRef<'a, R: RawIonReader + 'a> {
    fn read_value(&'a mut self) -> IonResult<ValueRef<'a, R>>;

    fn read_null(&'a mut self) -> IonResult<IonType> {
        self.read_value()?.expect_null()
    }

    fn read_bool(&'a mut self) -> IonResult<bool> {
        self.read_value()?.expect_bool()
    }

    fn read_int(&'a mut self) -> IonResult<Int> {
        self.read_value()?.expect_int()
    }

    fn read_i64(&'a mut self) -> IonResult<i64> {
        match self.read_value()?.expect_int()? {
            Int::I64(i) => Ok(i),
            Int::BigInt(i) => {
                i64::try_from(i).map_err(|_| decoding_error_raw("expected an i64, found a bigint"))
            }
        }
    }

    fn read_float(&'a mut self) -> IonResult<f64> {
        self.read_value()?.expect_float()
    }

    fn read_decimal(&'a mut self) -> IonResult<Decimal> {
        self.read_value()?.expect_decimal()
    }

    fn read_timestamp(&'a mut self) -> IonResult<Timestamp> {
        self.read_value()?.expect_timestamp()
    }

    fn read_string(&'a mut self) -> IonResult<&'a str> {
        self.read_value()?.expect_string()
    }

    fn read_symbol(&'a mut self) -> IonResult<Symbol> {
        self.read_value()?.expect_symbol()
    }

    fn read_clob(&'a mut self) -> IonResult<&'a [u8]> {
        self.read_value()?.expect_clob()
    }

    fn read_blob(&'a mut self) -> IonResult<&'a [u8]> {
        self.read_value()?.expect_blob()
    }

    fn as_list(&'a mut self) -> IonResult<SequenceRef<R>> {
        self.read_value()?.expect_list()
    }

    fn as_sexp(&'a mut self) -> IonResult<SequenceRef<R>> {
        self.read_value()?.expect_sexp()
    }

    fn as_struct(&'a mut self) -> IonResult<StructRef<R>> {
        self.read_value()?.expect_struct()
    }
}

impl<'a, R: RawIonReader + 'a> ReadValueRef<'a, R> for ValueReader<'a, R> {
    fn read_value(&'a mut self) -> IonResult<ValueRef<'a, R>> {
        ValueReader::read(self)
    }
}

impl<'a, R: RawIonReader + 'a> ReadValueRef<'a, R> for SystemReader<R> {
    fn read_value(&'a mut self) -> IonResult<ValueRef<'a, R>> {
        SystemReader::read_value(self)
    }
}
