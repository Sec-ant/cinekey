use cinekey::{ExpectedKind, ParseContext, ParsedMedia};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TYPES: &str = include_str!("types.d.ts");

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Context {
    expected_kind: Option<ExpectedKind>,
    ancestors: Vec<String>,
}

fn encode(media: &ParsedMedia) -> Result<JsValue, JsValue> {
    media
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(JsValue::from)
}

/// Parse a release name or the final component of a path string.
#[wasm_bindgen(js_name = parse, unchecked_return_type = "ParsedMedia")]
pub fn parse(input: &str) -> Result<JsValue, JsValue> {
    encode(&cinekey::parse(input))
}

/// Parse with a movie/series hint and nearest-first ancestor directory names.
#[wasm_bindgen(js_name = parseWithContext, unchecked_return_type = "ParsedMedia")]
pub fn parse_with_context(
    input: &str,
    #[wasm_bindgen(unchecked_param_type = "ParseContext")] context: JsValue,
) -> Result<JsValue, JsValue> {
    let context: Context = serde_wasm_bindgen::from_value(context).map_err(JsValue::from)?;
    let borrowed = ParseContext {
        expected_kind: context.expected_kind,
        ancestors: context.ancestors.iter().map(String::as_str).collect(),
    };
    encode(&cinekey::parse_with_context(input, &borrowed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    fn decode(value: JsValue) -> ParsedMedia {
        serde_wasm_bindgen::from_value(value).expect("metadata is a JavaScript object")
    }

    #[wasm_bindgen_test]
    fn metadata_round_trips_through_javascript() {
        for input in [
            "Arrival.2016.1080p.BluRay.mkv",
            "Example.S01E02-E04.1080p.WEB-DL.x264-GROUP.chs&eng.srt",
            "Amélie.2001.1080p.BluRay.x264-GROUP.mkv",
            "母亲的拥抱.A.Mothers.Embrace.2024.1080p.WEB-DL.mkv",
            "[Group] Example [NCED02][x265_flac].mkv",
            "",
        ] {
            assert_eq!(decode(parse(input).unwrap()), cinekey::parse(input));
        }
    }

    #[wasm_bindgen_test]
    fn ancestors_supply_title_and_season() {
        let value = serde_wasm_bindgen::to_value(&TestContext {
            expected_kind: ExpectedKind::Series,
            ancestors: ["Season 1", "Example.Show.S01.1080p"],
        })
        .unwrap();
        let media = decode(parse_with_context("E02.mkv", value).unwrap());
        assert_eq!(media.title.as_deref(), Some("Example Show"));
        assert_eq!(media.season, Some(1));
        assert_eq!(media.episodes, [2]);
    }

    #[derive(Serialize)]
    struct TestContext<'a> {
        expected_kind: ExpectedKind,
        ancestors: [&'a str; 2],
    }

    #[wasm_bindgen_test]
    fn malformed_context_returns_an_error() {
        assert!(parse_with_context("E02.mkv", JsValue::from_str("series")).is_err());
    }
}
