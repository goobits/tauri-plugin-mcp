use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

#[derive(Debug, Deserialize)]
pub struct DirectEvalRequest {
    pub code: String,
    pub window_label: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DirectEvalResponse {
    pub success: bool,
    pub result: Option<String>,
    pub error: Option<String>,
}

pub async fn handle_direct_eval<R: Runtime>(
    app: &AppHandle<R>,
    payload: serde_json::Value,
) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
    let request: DirectEvalRequest = serde_json::from_value(payload)?;
    let window_label = request.window_label.unwrap_or_else(|| "main".to_string());
    
    // Get the window
    let window = app
        .get_webview_window(&window_label)
        .ok_or_else(|| format!("Window '{}' not found", window_label))?;
    
    // Wrap the code to capture return value via a global variable
    let wrapped_code = format!(
        r#"
        (function() {{
            try {{
                const __result = (function() {{ {} }})();
                window.__mcpLastResult = {{
                    success: true,
                    value: __result,
                    type: typeof __result,
                    stringValue: String(__result)
                }};
                return __result;
            }} catch (e) {{
                window.__mcpLastResult = {{
                    success: false,
                    error: e.message,
                    stack: e.stack
                }};
                throw e;
            }}
        }})()
        "#,
        request.code
    );
    
    // Execute the wrapped code
    match window.eval(&wrapped_code) {
        Ok(_) => {
            // Now try to read back the result
            let read_result_code = r#"
                if (window.__mcpLastResult) {
                    JSON.stringify(window.__mcpLastResult);
                } else {
                    JSON.stringify({ success: true, value: undefined });
                }
            "#;
            
            // We can't get the result directly, but we've stored it
            Ok(serde_json::json!(DirectEvalResponse {
                success: true,
                result: Some("Code executed successfully. Result stored in window.__mcpLastResult".to_string()),
                error: None,
            }))
        }
        Err(e) => {
            Ok(serde_json::json!(DirectEvalResponse {
                success: false,
                result: None,
                error: Some(format!("Eval error: {}", e)),
            }))
        }
    }
}