use super::flatten_json_map;
use serde_json::json;

fn flatten(value: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    let mut value = value;
    let object = std::mem::take(value.as_object_mut().expect("object"));
    flatten_json_map(&object)
}

#[test]
fn flatten_json_map_preserves_empty_documents() {
    assert!(flatten(json!({})).is_empty());
}

#[test]
fn flatten_json_map_preserves_unflattened_documents() {
    let input = json!({
      "id": "287947",
      "title": "Shazam!",
      "release_date": 1553299200,
      "genres": ["Action", "Comedy", "Fantasy"]
    });

    assert_eq!(&flatten(input.clone()), input.as_object().unwrap());
}

#[test]
fn flatten_json_map_flattens_objects() {
    let flattened = flatten(json!({
      "a": {
        "b": "c",
        "d": "e",
        "f": "g"
      }
    }));

    assert_eq!(
        &flattened,
        json!({
            "a": {"b": "c", "d": "e", "f": "g"},
            "a.b": "c",
            "a.d": "e",
            "a.f": "g"
        })
        .as_object()
        .unwrap()
    );
}

#[test]
fn flatten_json_map_flattens_array_values() {
    let flattened = flatten(json!({
      "a": [
        1,
        "b",
        [],
        [{}],
        { "b": "c" },
        { "b": "d" },
        { "b": "e" }
      ]
    }));

    assert_eq!(
        &flattened,
        json!({
            "a": [1, "b"],
            "a.b": ["c", "d", "e"]
        })
        .as_object()
        .unwrap()
    );

    let flattened = flatten(json!({
      "a": [
        42,
        { "b": "c" },
        { "b": "d" },
        { "b": "e" }
      ]
    }));

    assert_eq!(
        &flattened,
        json!({
            "a": [42],
            "a.b": ["c", "d", "e"]
        })
        .as_object()
        .unwrap()
    );

    let flattened = flatten(json!({
      "a": [
        { "b": "c" },
        { "b": "d" },
        { "b": "e" },
        null
      ]
    }));

    assert_eq!(
        &flattened,
        json!({
            "a": [null],
            "a.b": ["c", "d", "e"]
        })
        .as_object()
        .unwrap()
    );
}

#[test]
fn flatten_json_map_preserves_collision_order() {
    let flattened = flatten(json!({
      "a": {
        "b": "c"
      },
      "a.b": "d"
    }));

    assert_eq!(
        &flattened,
        json!({
            "a": {"b": "c"},
            "a.b": ["c", "d"]
        })
        .as_object()
        .unwrap()
    );

    let flattened = flatten(json!({
      "a": [
        { "b": "c" },
        { "b": "d", "c": "e" },
        [35]
      ],
      "a.b": "f"
    }));

    assert_eq!(
        &flattened,
        json!({
            "a.b": ["c", "d", "f"],
            "a.c": "e",
            "a": [35]
        })
        .as_object()
        .unwrap()
    );
}

#[test]
fn flatten_json_map_flattens_nested_arrays() {
    let flattened = flatten(json!({
      "a": [
        ["b", "c"],
        { "d": "e" },
        ["f", "g"],
        [
            { "h": "i" },
            { "d": "j" }
        ],
        ["k", "l"]
      ]
    }));

    assert_eq!(
        &flattened,
        json!({
            "a": ["b", "c", "f", "g", "k", "l"],
            "a.d": ["e", "j"],
            "a.h": "i"
        })
        .as_object()
        .unwrap()
    );
}

#[test]
fn flatten_json_map_flattens_nested_arrays_and_objects() {
    let flattened = flatten(json!({
      "a": [
        "b",
        ["c", "d"],
        { "e": ["f", "g"] },
        [
            { "h": "i" },
            { "e": ["j", { "z": "y" }] }
        ],
        ["l"],
        "m"
      ]
    }));

    assert_eq!(
        &flattened,
        json!({
            "a": ["b", "c", "d", "l", "m"],
            "a.e": ["f", "g", "j"],
            "a.h": "i",
            "a.e.z": "y"
        })
        .as_object()
        .unwrap()
    );
}

#[test]
fn flatten_json_map_preserves_nested_original_values() {
    let flattened = flatten(json!({
        "tags": {
            "t1": "v1"
        },
        "prices": {
            "p1": [null],
            "p1000": {"tamo": {"le": {}}}
        },
        "kiki": [[]]
    }));

    assert_eq!(
        &flattened,
        json!({
          "prices": {
            "p1": [null],
            "p1000": {
              "tamo": {
                "le": {}
              }
            }
          },
          "prices.p1": [null],
          "prices.p1000": {
            "tamo": {
              "le": {}
            }
          },
          "prices.p1000.tamo": {
            "le": {}
          },
          "prices.p1000.tamo.le": {},
          "tags": {
            "t1": "v1"
          },
          "tags.t1": "v1",
          "kiki": [[]]
        })
        .as_object()
        .unwrap()
    );
}
