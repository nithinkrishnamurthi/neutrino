/// Tests for JSON <-> msgpack serialization used in HTTP handlers
///
/// These tests ensure that data can be correctly converted between:
/// - HTTP JSON requests -> msgpack (for sending to Python workers)
/// - msgpack results -> HTTP JSON responses (for returning to clients)

#[cfg(test)]
mod json_msgpack_conversion {
    use serde_json;
    use rmpv::Value as MsgpackValue;

    /// Convert serde_json::Value to rmpv::Value (same as in http/mod.rs)
    fn json_to_msgpack_value(json: &serde_json::Value) -> Result<MsgpackValue, String> {
        match json {
            serde_json::Value::Null => Ok(MsgpackValue::Nil),
            serde_json::Value::Bool(b) => Ok(MsgpackValue::Boolean(*b)),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(MsgpackValue::Integer(i.into()))
                } else if let Some(f) = n.as_f64() {
                    Ok(MsgpackValue::F64(f))
                } else {
                    Err("Invalid number".to_string())
                }
            }
            serde_json::Value::String(s) => Ok(MsgpackValue::String(s.clone().into())),
            serde_json::Value::Array(arr) => {
                let values: Result<Vec<_>, _> = arr.iter().map(json_to_msgpack_value).collect();
                Ok(MsgpackValue::Array(values?))
            }
            serde_json::Value::Object(obj) => {
                let pairs: Result<Vec<(MsgpackValue, MsgpackValue)>, String> = obj
                    .iter()
                    .map(|(k, v)| {
                        Ok((
                            MsgpackValue::String(k.clone().into()),
                            json_to_msgpack_value(v)?,
                        ))
                    })
                    .collect();
                Ok(MsgpackValue::Map(pairs?))
            }
        }
    }

    /// Convert rmpv::Value to serde_json::Value (same as in http/mod.rs)
    fn msgpack_value_to_json(msgpack: &MsgpackValue) -> Result<serde_json::Value, String> {
        match msgpack {
            MsgpackValue::Nil => Ok(serde_json::Value::Null),
            MsgpackValue::Boolean(b) => Ok(serde_json::Value::Bool(*b)),
            MsgpackValue::Integer(i) => {
                if let Some(val) = i.as_i64() {
                    Ok(serde_json::json!(val))
                } else if let Some(val) = i.as_u64() {
                    Ok(serde_json::json!(val))
                } else {
                    Err("Integer out of range".to_string())
                }
            }
            MsgpackValue::F32(f) => Ok(serde_json::json!(*f)),
            MsgpackValue::F64(f) => Ok(serde_json::json!(*f)),
            MsgpackValue::String(s) => Ok(serde_json::Value::String(
                s.as_str().ok_or("Invalid UTF-8")?.to_string(),
            )),
            MsgpackValue::Binary(b) => {
                // Convert binary to array of numbers for JSON compatibility
                Ok(serde_json::Value::Array(
                    b.iter().map(|&byte| serde_json::json!(byte)).collect(),
                ))
            }
            MsgpackValue::Array(arr) => {
                let values: Result<Vec<_>, _> = arr.iter().map(msgpack_value_to_json).collect();
                Ok(serde_json::Value::Array(values?))
            }
            MsgpackValue::Map(map) => {
                let mut obj = serde_json::Map::new();
                for (k, v) in map {
                    let key = match k {
                        MsgpackValue::String(s) => s.as_str().ok_or("Invalid UTF-8")?.to_string(),
                        _ => return Err("Map keys must be strings".to_string()),
                    };
                    obj.insert(key, msgpack_value_to_json(v)?);
                }
                Ok(serde_json::Value::Object(obj))
            }
            MsgpackValue::Ext(_, _) => Err("Extension types not supported".to_string()),
        }
    }

    #[test]
    fn test_null_roundtrip() {
        let json = serde_json::json!(null);
        let msgpack = json_to_msgpack_value(&json).unwrap();
        let back_to_json = msgpack_value_to_json(&msgpack).unwrap();
        assert_eq!(json, back_to_json);
    }

    #[test]
    fn test_boolean_roundtrip() {
        let json = serde_json::json!({"true_val": true, "false_val": false});
        let msgpack = json_to_msgpack_value(&json).unwrap();
        let back_to_json = msgpack_value_to_json(&msgpack).unwrap();
        assert_eq!(json, back_to_json);
    }

    #[test]
    fn test_integer_roundtrip() {
        let json = serde_json::json!({
            "small": 42,
            "negative": -123,
            "zero": 0,
            "large": 1234567890
        });
        let msgpack = json_to_msgpack_value(&json).unwrap();
        let back_to_json = msgpack_value_to_json(&msgpack).unwrap();
        assert_eq!(json, back_to_json);
    }

    #[test]
    fn test_float_roundtrip() {
        let json = serde_json::json!({
            "pi": 3.14159,
            "negative": -2.5,
            "scientific": 1.23e-4
        });
        let msgpack = json_to_msgpack_value(&json).unwrap();
        let back_to_json = msgpack_value_to_json(&msgpack).unwrap();

        // Compare floats with epsilon
        let obj = back_to_json.as_object().unwrap();
        assert!((obj["pi"].as_f64().unwrap() - 3.14159).abs() < 0.00001);
        assert_eq!(obj["negative"].as_f64().unwrap(), -2.5);
    }

    #[test]
    fn test_string_roundtrip() {
        let json = serde_json::json!({
            "simple": "hello",
            "unicode": "Hello 世界 🌍",
            "empty": "",
            "special": "Line1\nLine2\tTabbed"
        });
        let msgpack = json_to_msgpack_value(&json).unwrap();
        let back_to_json = msgpack_value_to_json(&msgpack).unwrap();
        assert_eq!(json, back_to_json);
    }

    #[test]
    fn test_array_roundtrip() {
        let json = serde_json::json!({
            "empty": [],
            "numbers": [1, 2, 3, 4, 5],
            "mixed": [1, "two", 3.0, true, null],
            "nested": [[1, 2], [3, 4], [5, 6]]
        });
        let msgpack = json_to_msgpack_value(&json).unwrap();
        let back_to_json = msgpack_value_to_json(&msgpack).unwrap();
        assert_eq!(json, back_to_json);
    }

    #[test]
    fn test_nested_object_roundtrip() {
        let json = serde_json::json!({
            "user": {
                "id": 1,
                "name": "Alice",
                "metadata": {
                    "created": "2024-01-01",
                    "tags": ["admin", "user"]
                }
            }
        });
        let msgpack = json_to_msgpack_value(&json).unwrap();
        let back_to_json = msgpack_value_to_json(&msgpack).unwrap();
        assert_eq!(json, back_to_json);
    }

    #[test]
    fn test_task_request_conversion() {
        // Simulate HTTP POST body for task execution
        let json = serde_json::json!({
            "text": "hello world",
            "iterations": 1000,
            "options": {
                "timeout": 30,
                "retries": 3
            }
        });

        let msgpack = json_to_msgpack_value(&json).unwrap();

        // Verify msgpack structure
        assert!(matches!(msgpack, MsgpackValue::Map(_)));

        let back_to_json = msgpack_value_to_json(&msgpack).unwrap();
        assert_eq!(json, back_to_json);
    }

    #[test]
    fn test_task_result_conversion() {
        // Simulate worker response
        let result_data = serde_json::json!({
            "result": "olleh dlrow",
            "processed_chars": 11000
        });

        let msgpack = json_to_msgpack_value(&result_data).unwrap();
        let back_to_json = msgpack_value_to_json(&msgpack).unwrap();

        assert_eq!(back_to_json["result"], "olleh dlrow");
        assert_eq!(back_to_json["processed_chars"], 11000);
    }

    #[test]
    fn test_error_result_conversion() {
        // Simulate error response from worker
        let error_data = serde_json::json!({
            "error": "ValueError: invalid input",
            "type": "ValueError"
        });

        let msgpack = json_to_msgpack_value(&error_data).unwrap();
        let back_to_json = msgpack_value_to_json(&msgpack).unwrap();

        assert_eq!(back_to_json["type"], "ValueError");
        assert!(back_to_json["error"].as_str().unwrap().contains("ValueError"));
    }

    #[test]
    fn test_large_array() {
        // Test with large dataset
        let numbers: Vec<i64> = (0..10000).collect();
        let json = serde_json::json!({"data": numbers});

        let msgpack = json_to_msgpack_value(&json).unwrap();
        let back_to_json = msgpack_value_to_json(&msgpack).unwrap();

        assert_eq!(back_to_json["data"].as_array().unwrap().len(), 10000);
        assert_eq!(back_to_json["data"][9999], 9999);
    }

    #[test]
    fn test_binary_to_json() {
        // Binary data converts to array of numbers in JSON
        let binary = MsgpackValue::Binary(vec![0x00, 0x01, 0x02, 0xff]);
        let json = msgpack_value_to_json(&binary).unwrap();

        assert_eq!(json, serde_json::json!([0, 1, 2, 255]));
    }

    #[test]
    fn test_unicode_edge_cases() {
        let json = serde_json::json!({
            "emoji": "😀😃😄😁",
            "chinese": "你好世界",
            "arabic": "مرحبا بالعالم",
            "mixed": "Hello 世界 🌍"
        });

        let msgpack = json_to_msgpack_value(&json).unwrap();
        let back_to_json = msgpack_value_to_json(&msgpack).unwrap();
        assert_eq!(json, back_to_json);
    }

    #[test]
    fn test_empty_structures() {
        let json = serde_json::json!({
            "empty_object": {},
            "empty_array": [],
            "empty_string": ""
        });

        let msgpack = json_to_msgpack_value(&json).unwrap();
        let back_to_json = msgpack_value_to_json(&msgpack).unwrap();
        assert_eq!(json, back_to_json);
    }

    #[test]
    fn test_special_floats() {
        // Test infinity - note that serde_json may serialize infinity as null
        let msgpack_inf = MsgpackValue::F64(f64::INFINITY);
        let json_inf = msgpack_value_to_json(&msgpack_inf).unwrap();
        // JSON doesn't have a standard representation for infinity, so it may be null
        // or the f64 value depending on serde_json settings
        if let Some(f) = json_inf.as_f64() {
            assert!(f.is_infinite());
        }

        let msgpack_small = MsgpackValue::F64(1e-308);
        let json_small = msgpack_value_to_json(&msgpack_small).unwrap();
        assert_eq!(json_small.as_f64().unwrap(), 1e-308);
    }

    #[test]
    fn test_invalid_map_key() {
        // Map keys must be strings in JSON
        let invalid_map = MsgpackValue::Map(vec![
            (MsgpackValue::Integer(123.into()), MsgpackValue::String("value".into()))
        ]);

        let result = msgpack_value_to_json(&invalid_map);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Map keys must be strings"));
    }

    #[test]
    fn test_deeply_nested() {
        // Create deeply nested structure
        let mut nested = serde_json::json!({"level": 1});
        let mut current = &mut nested;

        for i in 2..50 {
            let new_level = serde_json::json!({"level": i});
            current.as_object_mut().unwrap().insert("nested".to_string(), new_level);
            current = &mut current["nested"];
        }

        let msgpack = json_to_msgpack_value(&nested).unwrap();
        let back_to_json = msgpack_value_to_json(&msgpack).unwrap();

        // Verify deepest level
        let mut current = &back_to_json;
        for _ in 0..48 {
            current = &current["nested"];
        }
        assert_eq!(current["level"], 49);
    }
}
