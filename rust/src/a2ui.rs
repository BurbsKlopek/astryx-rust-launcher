use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Pure-Rust A2UI v1.0 Protocol Engine
/// Processes createSurface, updateComponents, updateDataModel, and deleteSurface messages.
#[derive(Debug, Clone, Default)]
pub struct A2uiEngine {
    pub surfaces: BTreeMap<String, A2uiSurfaceState>,
}

#[derive(Debug, Clone)]
pub struct A2uiSurfaceState {
    pub surface_id: String,
    pub catalog_id: String,
    pub components: BTreeMap<String, A2uiComponent>,
    pub data_model: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2uiComponent {
    pub id: String,
    pub component: String,
    #[serde(flatten)]
    pub props: BTreeMap<String, Value>,
}

impl A2uiEngine {
    pub fn process_json(&mut self, json: &str) -> Result<usize, String> {
        let val: Value = serde_json::from_str(json).map_err(|e| format!("Invalid JSON: {e}"))?;
        let list = match val {
            Value::Array(arr) => arr,
            other => vec![other],
        };
        let mut count = 0;
        for msg in list {
            self.process_message(&msg)?;
            count += 1;
        }
        Ok(count)
    }

    pub fn process_message(&mut self, msg: &Value) -> Result<(), String> {
        if let Some(cs) = msg.get("createSurface") {
            let sid = cs
                .get("surfaceId")
                .and_then(|v| v.as_str())
                .ok_or("createSurface missing surfaceId")?;
            let cid = cs
                .get("catalogId")
                .and_then(|v| v.as_str())
                .unwrap_or("https://astryxtv.local/catalogs/tv-v1.json");
            self.surfaces.insert(
                sid.to_string(),
                A2uiSurfaceState {
                    surface_id: sid.to_string(),
                    catalog_id: cid.to_string(),
                    components: BTreeMap::new(),
                    data_model: Value::Object(serde_json::Map::new()),
                },
            );
        }
        if let Some(uc) = msg.get("updateComponents") {
            let sid = uc
                .get("surfaceId")
                .and_then(|v| v.as_str())
                .ok_or("updateComponents missing surfaceId")?;
            let surface = self
                .surfaces
                .get_mut(sid)
                .ok_or_else(|| format!("Surface '{sid}' does not exist"))?;
            if let Some(comps) = uc.get("components").and_then(|v| v.as_array()) {
                for c in comps {
                    let parsed: A2uiComponent = serde_json::from_value(c.clone())
                        .map_err(|e| format!("Invalid component: {e}"))?;
                    surface.components.insert(parsed.id.clone(), parsed);
                }
            }
        }
        if let Some(ud) = msg.get("updateDataModel") {
            let sid = ud
                .get("surfaceId")
                .and_then(|v| v.as_str())
                .ok_or("updateDataModel missing surfaceId")?;
            let surface = self
                .surfaces
                .get_mut(sid)
                .ok_or_else(|| format!("Surface '{sid}' does not exist"))?;
            if let Some(val) = ud.get("value") {
                surface.data_model = val.clone();
            }
        }
        if let Some(ds) = msg.get("deleteSurface") {
            if let Some(sid) = ds.get("surfaceId").and_then(|v| v.as_str()) {
                self.surfaces.remove(sid);
            }
        }
        Ok(())
    }

    pub fn clear_custom(&mut self) {
        self.surfaces.retain(|k, _| k == "tv_home");
    }
}

pub const SAMPLE_A2UI_PAYLOAD: &str = r#"[
  {
    "version": "1.0",
    "createSurface": {
      "surfaceId": "agent_tonight",
      "catalogId": "https://astryxtv.local/catalogs/tv-v1.json"
    }
  },
  {
    "version": "1.0",
    "updateComponents": {
      "surfaceId": "agent_tonight",
      "components": [
        {
          "id": "root",
          "component": "TvShelf",
          "title": "Agent Surface · Tonight on TV",
          "children": ["card_cinema", "card_network"]
        },
        {
          "id": "card_cinema",
          "component": "TvWidgetCard",
          "title": "4K Cinema Mode",
          "value": "2160p Ready",
          "subtitle": "Pure Native Rust OpenGL ES"
        },
        {
          "id": "card_network",
          "component": "TvWidgetCard",
          "title": "A2UI v1.0 Native",
          "value": "Zero WebView",
          "subtitle": "Streamed directly into Rust GPU UI"
        }
      ]
    }
  }
]"#;
