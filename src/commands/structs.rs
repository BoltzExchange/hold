use serde::de::DeserializeOwned;
use serde_json::Value;
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub enum ParamsError {
    ParseError,
    TooFewParams,
    TooManyParams,
}

impl Display for ParamsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match *self {
            ParamsError::ParseError => write!(f, "could not parse request"),
            ParamsError::TooFewParams => write!(f, "too few parameters"),
            ParamsError::TooManyParams => write!(f, "too many parameters"),
        }
    }
}

impl Error for ParamsError {}

pub trait FromArr {
    fn from_arr(arr: Vec<Value>) -> anyhow::Result<Self>
    where
        Self: Sized;
}

pub fn parse_args<T>(args: Value) -> anyhow::Result<T>
where
    T: DeserializeOwned + FromArr,
{
    if let Value::Array(arr) = args {
        return T::from_arr(arr);
    }

    Ok(serde_json::from_value(args)?)
}
