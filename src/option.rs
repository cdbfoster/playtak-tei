use std::io;
use std::ops::RangeInclusive;
use std::str::FromStr;

use async_std::io::WriteExt;
use tracing::{debug, error, warn};

use super::{err, write};

#[derive(Debug)]
pub struct SpinOption {
    pub name: String,
    pub default: i32,
    pub range: RangeInclusive<i32>,
}

impl FromStr for SpinOption {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut option = SpinOption {
            name: String::new(),
            default: 0,
            range: 0..=0,
        };

        let mut parts = s.split_ascii_whitespace();
        while let Some(part) = parts.next() {
            match part {
                "name" => option.name = parts.next().ok_or("expected option name")?.to_owned(),
                "type" => assert_eq!(parts.next(), Some("spin"), "expected type spin"),
                "default" => {
                    option.default = parts
                        .next()
                        .ok_or("expected option default")?
                        .parse::<i32>()
                        .map_err(|_| "expected an integer")?
                }
                "min" => {
                    option.range = parts
                        .next()
                        .ok_or("expected option min")?
                        .parse::<i32>()
                        .map_err(|_| "expected an integer")?
                        ..=*option.range.end()
                }
                "max" => {
                    option.range = *option.range.start()
                        ..=parts
                            .next()
                            .ok_or("expected option max")?
                            .parse::<i32>()
                            .map_err(|_| "expected an integer")?
                }
                _ => (),
            }
        }

        Ok(option)
    }
}

impl SpinOption {
    pub fn valid_value(&self, value: i32) -> bool {
        self.range.contains(&value)
    }

    pub fn to_tei_string(&self, value: i32) -> String {
        if !self.valid_value(value) {
            warn!(option = ?self.name, ?value, range = ?self.range, "Attempting to set TEI option to an invalid value.");
        }

        format!("setoption name {} value {}\n", self.name, value)
    }
}

pub async fn validate_and_set_option(
    writer: impl WriteExt + Unpin,
    options: &[SpinOption],
    name: &str,
    value: i32,
    default: i32, // A global default to use if the engine doesn't provide its own.
) -> io::Result<()> {
    if let Some(option) = options.iter().find(|o| o.name == name) {
        if value != option.default {
            write(writer, option.to_tei_string(value)).await?;
        } else {
            debug!(
                "Requested option \"{name}\" is already at the engine's default value. Skipping configuration."
            )
        }
    } else if value != default {
        error!("Requested option \"{name}\" is not at the assumed default value, and the engine doesn't support the configuration.");
        return Err(err!());
    }

    Ok(())
}
