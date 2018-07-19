use nom::{self, types::CompleteStr as Input, IResult};

use serde_json::Value;

#[derive(Debug)]
pub enum Schema<'a> {
    Tag(&'a str),
    Obj(&'a str, Vec<Schema<'a>>),
    Root(Vec<Schema<'a>>),
}

impl<'a> ToString for Schema<'a> {
    fn to_string(&self) -> String {
        match self {
            Schema::Tag(n) => n.trim().to_string(),
            Schema::Obj(k, _) => k.trim().to_string(),
            Schema::Root(_) => "$".trim().to_string(),
        }
    }
}

fn value_tag_parser(c: Input) -> IResult<Input, Schema> {
    let c_i = c.find(',').unwrap_or(usize::max_value() - 1);
    let b_i = c.find('{').unwrap_or(usize::max_value());
    let b_amount = c.chars().fold(1, |acc, i| match i {
        '{' => acc + 1,
        '}' => acc - 1,
        _ => acc,
    });
    if b_amount <= 1 && c_i < b_i {
        let (i, o) = if c.find(',').is_some() {
            take_until!(c, ",")?
        } else if c.find('}').is_some() {
            take_until!(c, "}")?
        } else {
            ("".into(), c)
        };
        Ok((i, Schema::Tag(&o)))
    } else {
        Err(nom::Err::Error(nom::Context::Code(c, nom::ErrorKind::Tag)))
    }
}

fn value_obj_parser(c: Input) -> IResult<Input, Schema> {
    let (c, n) = take_until!(c, "{")?;
    match obj_parser(c) {
        Ok((c, Schema::Root(d))) => Ok((c, Schema::Obj(&n.trim(), d))),
        _ => Err(nom::Err::Error(nom::Context::Code(
            c,
            nom::ErrorKind::Custom(line!()),
        ))),
    }
}

named!(value_parser<Input, Vec<Schema>>,
    separated_list!(
        tag!(","),
        alt!(
            value_obj_parser |
            value_tag_parser
        )
    )
);

fn obj_parser(i: Input) -> IResult<Input, Schema> {
    let (i, _) = tag!(i, "{")?;
    let mut n = 0;
    let mut acc = 0;
    for (i, e) in i.chars().enumerate() {
        let v = match e {
            '{' => acc + 1,
            '}' => acc - 1,
            _ => acc,
        };
        if v < 0 {
            n = i;
            break;
        } else {
            acc = v;
        }
    }
    let (i, value_v) = take!(i, n)?;
    let (_, v) = value_parser(value_v)?;
    let (i, _) = tag!(i, "}")?;
    Ok((i, Schema::Root(v)))
}

#[inline(always)]
pub fn schema_parser<'a>(i: &'a str) -> Result<Schema<'a>, nom::Err<Input>> {
    let (_, v) = ws!(Input(i), obj_parser)?;
    Ok(v)
}

impl<'a> Schema<'a> {
    pub fn as_schema(&self, v: &Value) -> Option<Value> {
        match self {
            Schema::Tag(k) => match v.get(k.trim()) {
                Some(value) => Some(value.clone()),
                None => None,
            },
            Schema::Root(vec) => Some(Value::Object(
                vec.iter()
                    .filter_map(|e| {
                        let name = match e {
                            Schema::Tag(k) => k.trim().to_string(),
                            Schema::Obj(k, _) => k.trim().to_string(),
                            _ => unreachable!("root in root"),
                        };
                        match e.as_schema(v) {
                            Some(v) => Some((name, v)),
                            None => None,
                        }
                    })
                    .collect(),
            )),
            Schema::Obj(k, c) => match v.get(k.trim()) {
                Some(value) => Some(Value::Object(
                    c.iter()
                        .filter_map(|e| match e.as_schema(value) {
                            Some(f) => Some((e, f)),
                            None => None,
                        })
                        .map(|(e, f)| (e.to_string(), f))
                        .collect(),
                )),
                None => unreachable!("None Object"),
            },
        }
    }
}

pub fn merge(schema: Value, data: Value) -> Value {
    match (schema, data) {
        (Value::String(_), Value::String(s2)) => Value::String(s2),
        (Value::Null, Value::Null) => Value::Null,
        (Value::Bool(_), Value::Bool(s2)) => Value::Bool(s2),
        (Value::Array(v), Value::Array(v2)) => Value::Array(
            v.into_iter()
                .zip(v2.into_iter())
                .map(|(v1, v2)| merge(v1, v2))
                .collect(),
        ),
        (Value::Object(map), Value::Object(mut map2)) => Value::Object(
            map.into_iter()
                .map(|(k1, v1)| {
                    if map2.contains_key(&k1) {
                        let v2 = map2.remove(&k1).unwrap();
                        (k1, merge(v1, v2))
                    } else {
                        (k1, v1)
                    }
                })
                .collect(),
        ),
        (Value::Number(_), Value::Number(n2)) => Value::Number(n2),
        _ => unreachable!(),
    }
}
