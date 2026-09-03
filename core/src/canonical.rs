//! The JSON canonicalization every signature covers (PROTOCOL.md §2).
//!
//! Must reproduce OpenVole's `canonicalJson` exactly: object keys sorted recursively, members
//! whose value is `undefined` omitted (there is no `undefined` in serde — absent is absent),
//! and everything else as `JSON.stringify` would print it. The one place JSON.stringify and
//! serde disagree is non-integer numbers, so payloads this crate *creates* only ever contain
//! strings, integers, booleans, arrays and objects; integers are printed as plain digits, which
//! both agree on.

use serde_json::Value;

/// Canonical JSON text for a value.
pub fn canonical_json(v: &Value) -> String {
	let mut out = String::new();
	write(v, &mut out);
	out
}

fn write(v: &Value, out: &mut String) {
	match v {
		Value::Null => out.push_str("null"),
		Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
		Value::Number(n) => write_number(n, out),
		Value::String(s) => out.push_str(&serde_json::to_string(s).expect("string serializes")),
		Value::Array(items) => {
			out.push('[');
			for (i, item) in items.iter().enumerate() {
				if i > 0 {
					out.push(',');
				}
				write(item, out);
			}
			out.push(']');
		}
		Value::Object(map) => {
			let mut keys: Vec<&String> = map.keys().collect();
			keys.sort();
			out.push('{');
			for (i, k) in keys.iter().enumerate() {
				if i > 0 {
					out.push(',');
				}
				out.push_str(&serde_json::to_string(k).expect("key serializes"));
				out.push(':');
				write(&map[*k], out);
			}
			out.push('}');
		}
	}
}

/// Numbers the way JavaScript prints them, for the range that matters: integers as digits,
/// integral floats without a fractional part, everything else via the shortest round-trip
/// form (which JS and Rust agree on for ordinary magnitudes).
fn write_number(n: &serde_json::Number, out: &mut String) {
	if let Some(i) = n.as_i64() {
		out.push_str(&i.to_string());
	} else if let Some(u) = n.as_u64() {
		out.push_str(&u.to_string());
	} else if let Some(f) = n.as_f64() {
		if f.is_finite() && f.fract() == 0.0 && f.abs() < 1e21 {
			out.push_str(&format!("{}", f as i128));
		} else {
			out.push_str(&f.to_string());
		}
	} else {
		out.push_str("null");
	}
}

/// The exact string a message signature covers.
pub fn signed_string(id: &str, r#type: &str, from: &str, to: &str, timestamp: i64, payload: &Value) -> String {
	let payload_str = if payload.is_null() { String::new() } else { canonical_json(payload) };
	format!("{}:{id}:{type}:{from}:{to}:{timestamp}:{payload_str}", crate::VOLENET_VERSION, type = r#type)
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	#[test]
	fn keys_sort_recursively() {
		let v = json!({"b": 1, "a": {"z": [3, {"y": true, "x": null}], "m": "s"}});
		assert_eq!(canonical_json(&v), r#"{"a":{"m":"s","z":[3,{"x":null,"y":true}]},"b":1}"#);
	}

	#[test]
	fn strings_escape_like_json_stringify() {
		let v = json!({"t": "line\nbreak \"q\" \\ \u{1f}"});
		assert_eq!(canonical_json(&v), r#"{"t":"line\nbreak \"q\" \\ \u001f"}"#);
	}

	#[test]
	fn null_payload_signs_as_empty() {
		assert_eq!(signed_string("i", "ping", "a", "b", 5, &Value::Null), "2:i:ping:a:b:5:");
		assert_eq!(signed_string("i", "ping", "a", "b", 5, &json!({"timestamp": 5})), "2:i:ping:a:b:5:{\"timestamp\":5}");
	}
}
