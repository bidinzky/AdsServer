use nom::{self, types::CompleteStr as Input, IResult};
use serde_json::{Map, Value};

#[derive(Debug, PartialEq, Clone)]
pub enum Schema {
    Tag(String),
    Obj(String, Vec<Schema>),
    Root(Vec<Schema>),
}

#[derive(Debug)]
pub enum Either {
    Resolve(Schema),
    Subscription(Schema),
}

impl ToString for Schema {
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
        Ok((i, Schema::Tag(o.to_string())))
    } else {
        Err(nom::Err::Error(nom::Context::Code(c, nom::ErrorKind::Tag)))
    }
}

fn value_obj_parser(c: Input) -> IResult<Input, Schema> {
    let (c, n) = take_until!(c, "{")?;
    match obj_parser(c) {
        Ok((c, Schema::Root(d))) => Ok((c, Schema::Obj(n.trim().to_string(), d))),
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
    let (_, v) = value_parser(Input(value_v.trim()))?;
    let (i, _) = tag!(i, "}")?;
    Ok((i, Schema::Root(v)))
}

pub fn schema_parser(i: &str) -> Either {
    let s: String = i.chars().filter(|x| *x != ' ').collect();
    if s[..12].eq_ignore_ascii_case("SUBSCRIPTION") {
        let i = s.find('{').unwrap();
        let (_, v) = ws!(Input(&s[i..]), obj_parser).unwrap();
        Either::Subscription(v)
    } else {
        let (_, v) = ws!(Input(&s), obj_parser).unwrap();
        Either::Resolve(v)
    }
}

impl Schema {
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
            Schema::Obj(k, c) => Some(Value::Object({
                let v = Value::Object(
                    c.iter()
                        .filter_map(|e| match e.as_schema(&v[k.trim()]) {
                            Some(f) => Some((e, f)),
                            None => None,
                        })
                        .map(|(e, f)| (e.to_string(), f))
                        .collect(),
                );
                let mut m = Map::new();
                m.insert(k.trim().to_string(), v);
                m
            })),
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

pub fn merge_schemas(value1: Schema, value2: Schema) -> Schema {
    match (value1, value2) {
        (Schema::Tag(_), Schema::Tag(s)) => Schema::Tag(s),
        (Schema::Root(v1), Schema::Root(v2)) => Schema::Root(
            v1.into_iter()
                .zip(v2.into_iter())
                .flat_map(|(v1, v2)| match (v1, v2) {
                    (Schema::Tag(s1), Schema::Tag(s2)) => {
                        if s1 == s2 {
                            vec![Schema::Tag(s1)]
                        } else {
                            vec![Schema::Tag(s1), Schema::Tag(s2)]
                        }
                    }
                    (v1, v2) => vec![merge_schemas(v1, v2)],
                })
                .collect(),
        ),
        (Schema::Obj(k1, v1), Schema::Obj(k2, v2)) => {
            if k1 == k2 {
                if let Schema::Root(d) = merge_schemas(Schema::Root(v1), Schema::Root(v2)) {
                    Schema::Obj(k1, d)
                } else {
                    unimplemented!()
                }
            } else {
                Schema::Root(vec![Schema::Obj(k1, v1), Schema::Obj(k2, v2)])
            }
        }
        _ => unreachable!(),
    }
}

pub fn merge_values(value1: Vec<Value>) -> Value {
    if value1.is_empty() {
        Value::Null
    } else {
        let mut data = Map::new();
        for v in value1 {
            if let Value::Object(map) = v {
                for (k, v) in map {
                    data.insert(k, v);
                }
            }
        }
        Value::Object(data)
    }
}
