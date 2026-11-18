use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
    Date(SystemTime),
    List(Vec<Value>),
    Map(Vec<(String, Value)>),
}

impl Value {
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Number(n) => *n != 0.0,
            Value::Text(s) => !s.is_empty(),
            Value::Date(_) => true,
            Value::List(items) => !items.is_empty(),
            Value::Map(entries) => !entries.is_empty(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Value::Null => true,
            Value::Text(s) => s.is_empty(),
            Value::List(items) => items.is_empty(),
            Value::Map(entries) => entries.is_empty(),
            Value::Number(n) => n.is_nan(),
            Value::Bool(_) | Value::Date(_) => false,
        }
    }
}

impl From<&focus_core::PropValue> for Value {
    fn from(value: &focus_core::PropValue) -> Self {
        use focus_core::PropValue;
        match value {
            PropValue::Null => Value::Null,
            PropValue::Bool(b) => Value::Bool(*b),
            PropValue::Number(n) => Value::Number(*n),
            PropValue::Text(s) => Value::Text(s.clone()),
            PropValue::List(items) => Value::List(items.iter().map(Value::from).collect()),
            PropValue::Map(entries) => Value::Map(
                entries
                    .iter()
                    .map(|(k, v)| (k.clone(), Value::from(v)))
                    .collect(),
            ),
        }
    }
}
