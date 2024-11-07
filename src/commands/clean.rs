use crate::commands::structs::{parse_args, FromArr, ParamsError};
use crate::database::helpers::invoice_helper::InvoiceHelper;
use crate::encoder::InvoiceEncoder;
use crate::State;
use cln_plugin::Plugin;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
struct CleanRequest {
    age: Option<u64>,
}

impl FromArr for CleanRequest {
    fn from_arr(arr: Vec<Value>) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        if arr.is_empty() {
            return Ok(Self { age: None });
        }

        Ok(Self {
            age: Some(arr[0].as_u64().ok_or(ParamsError::ParseError)?),
        })
    }
}

#[derive(Debug, Serialize)]
struct CleanResponse {
    pub cleaned: usize,
}

pub async fn clean<T, E>(plugin: Plugin<State<T, E>>, args: Value) -> anyhow::Result<Value>
where
    T: InvoiceHelper + Sync + Send + Clone,
    E: InvoiceEncoder + Sync + Send + Clone,
{
    let params = parse_args::<CleanRequest>(args)?;

    let cleaned = plugin.state().invoice_helper.clean_cancelled(params.age)?;

    Ok(serde_json::to_value(&CleanResponse { cleaned })?)
}
