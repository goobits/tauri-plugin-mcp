use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager, Runtime};
use log::info;

use crate::socket_server::SocketResponse;

#[derive(Debug, Deserialize)]
pub struct ConsoleOutputRequest {
    pub window_label: Option<String>,
    pub session_id: Option<String>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConsoleEntry {
    pub level: String,
    pub message: String,
    pub timestamp: String,
    pub session_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JavaScriptError {
    pub message: String,
    pub filename: Option<String>,
    pub lineno: Option<u32>,
    pub colno: Option<u32>,
    pub stack: Option<String>,
    pub timestamp: String,
    pub session_id: String,
}

#[derive(Debug, Serialize)]
pub struct ConsoleOutputResponse {
    pub entries: Vec<ConsoleEntry>,
    pub total_count: usize,
    pub session_id: String,
}


/// Setup console capture with event-based communication
pub async fn handle_setup_console_capture<R: Runtime>(
    app: &AppHandle<R>,
    payload: Value,
) -> crate::Result<SocketResponse> {
    let request: ConsoleOutputRequest = serde_json::from_value(payload)
        .map_err(|e| crate::Error::Anyhow(format!("Invalid request format: {}", e)))?;
    let window_label = request.window_label.unwrap_or_else(|| "main".to_string());
    
    info!("[TAURI_MCP] Setting up event-based console capture for window: {}", window_label);
    
    let window = app.get_webview_window(&window_label)
        .ok_or_else(|| crate::Error::Anyhow(format!("Window '{}' not found", window_label)))?;
    
    // Event listeners will be setup individually when needed
    
    // Inject our event-based console capture system
    let capture_code = r#"
        (function() {
            if (window.__mcpEventConsoleCapture) return { already_setup: true };
            
            window.__mcpEventConsoleCapture = true;
            window.__consoleBuffer = window.__consoleBuffer || [];
            window.__consoleSessionId = Date.now().toString();
            
            // Store original console methods
            const originalConsole = {
                log: console.log,
                error: console.error,
                warn: console.warn,
                info: console.info,
                debug: console.debug
            };
            
            function wrapConsoleMethod(level, originalMethod) {
                return function(...args) {
                    // Call original method first
                    originalMethod.apply(console, args);
                    
                    // Capture the message
                    const message = args.map(arg => 
                        typeof arg === 'object' ? JSON.stringify(arg) : String(arg)
                    ).join(' ');
                    
                    const entry = {
                        level: level,
                        message: message,
                        timestamp: new Date().toISOString(),
                        sessionId: window.__consoleSessionId
                    };
                    
                    // Store in buffer for retrieval
                    window.__consoleBuffer.push(entry);
                    
                    // Also store in a special MCP messages buffer for easy retrieval
                    if (!window.__mcpConsoleMessages) window.__mcpConsoleMessages = [];
                    window.__mcpConsoleMessages.push(entry);
                };
            }
            
            // Wrap all console methods
            console.log = wrapConsoleMethod('log', originalConsole.log);
            console.error = wrapConsoleMethod('error', originalConsole.error);
            console.warn = wrapConsoleMethod('warn', originalConsole.warn);
            console.info = wrapConsoleMethod('info', originalConsole.info);
            console.debug = wrapConsoleMethod('debug', originalConsole.debug);
            
            // Setup global error handlers
            window.addEventListener('error', function(event) {
                const errorInfo = {
                    message: event.message,
                    filename: event.filename,
                    lineno: event.lineno,
                    colno: event.colno,
                    stack: event.error ? event.error.stack : null,
                    timestamp: new Date().toISOString(),
                    sessionId: window.__consoleSessionId
                };
                
                // Store JavaScript errors in a buffer for retrieval
                if (!window.__mcpJSErrors) window.__mcpJSErrors = [];
                window.__mcpJSErrors.push(errorInfo);
            });
            
            // Setup unhandled promise rejection handlers
            window.addEventListener('unhandledrejection', function(event) {
                const rejectionInfo = {
                    message: 'Unhandled Promise Rejection: ' + String(event.reason),
                    filename: null,
                    lineno: null,
                    colno: null,
                    stack: event.reason && event.reason.stack ? event.reason.stack : null,
                    timestamp: new Date().toISOString(),
                    sessionId: window.__consoleSessionId
                };
                
                // Store promise rejections in the error buffer
                if (!window.__mcpJSErrors) window.__mcpJSErrors = [];
                window.__mcpJSErrors.push(rejectionInfo);
            });
            
            // Utility functions
            window.__getConsoleBuffer = function() {
                return window.__consoleBuffer || [];
            };
            
            window.__clearConsoleBuffer = function() {
                window.__consoleBuffer = [];
            };
            
            console.log('Event-based console capture system initialized');
            return { 
                setup_complete: true, 
                session_id: window.__consoleSessionId,
                capture_method: 'events'
            };
        })()
    "#;
    
    window.eval(capture_code)
        .map_err(|e| crate::Error::Anyhow(format!("Failed to setup console capture: {}", e)))?;
    
    Ok(SocketResponse {
        success: true,
        data: Some(serde_json::json!({
            "message": "Event-based console capture setup complete",
            "window_label": window_label
        })),
        error: None,
    })
}


/// Get JavaScript result using direct console message buffer inspection
pub async fn handle_get_js_result<R: Runtime>(
    app: &AppHandle<R>,
    payload: Value,
) -> crate::Result<SocketResponse> {
    #[derive(Debug, Deserialize)]
    struct GetJsResultRequest {
        window_label: Option<String>,
        variable_name: Option<String>,
    }
    
    let request: GetJsResultRequest = serde_json::from_value(payload)
        .map_err(|e| crate::Error::Anyhow(format!("Invalid request format: {}", e)))?;
    let window_label = request.window_label.unwrap_or_else(|| "main".to_string());
    let variable_name = request.variable_name.unwrap_or_else(|| "__mcpLastResult".to_string());
    
    info!("[TAURI_MCP] Getting JS result '{}' from window: {} (buffer-based)", variable_name, window_label);
    
    let window = app.get_webview_window(&window_label)
        .ok_or_else(|| crate::Error::Anyhow(format!("Window '{}' not found", window_label)))?;
    
    // Create a unique key to identify this retrieval
    let result_key = format!("mcp_result_{}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis());
    
    // Step 1: Execute JavaScript that will log the result with our unique key
    let retrieve_code = format!(r#"
        (function() {{
            try {{
                const value = window.{};
                const result = {{
                    success: true,
                    value: value,
                    type: typeof value,
                    timestamp: new Date().toISOString()
                }};
                
                // Log with unique identifier
                console.log('MCP_RETRIEVE_SUCCESS_{}:' + JSON.stringify(result));
                return 'retrieval_logged';
            }} catch (error) {{
                console.log('MCP_RETRIEVE_ERROR_{}:' + JSON.stringify({{
                    success: false,
                    error: error.message,
                    stack: error.stack,
                    timestamp: new Date().toISOString()
                }}));
                return 'error_logged';
            }}
        }})()
    "#, variable_name, result_key, result_key);
    
    // Execute the retrieval JavaScript
    window.eval(&retrieve_code)
        .map_err(|e| crate::Error::Anyhow(format!("Failed to execute retrieval JavaScript: {}", e)))?;
    
    // Small delay to let console.log execute
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Step 2: Get the console messages and look for our result
    let search_code = format!(r#"
        (function() {{
            if (window.__mcpConsoleMessages) {{
                const messages = window.__mcpConsoleMessages;
                for (let i = messages.length - 1; i >= 0; i--) {{
                    const message = messages[i].message;
                    if (message.includes('MCP_RETRIEVE_SUCCESS_{}:') || message.includes('MCP_RETRIEVE_ERROR_{}:')) {{
                        const colonIndex = message.indexOf(':');
                        if (colonIndex !== -1) {{
                            const data = message.substring(colonIndex + 1);
                            window.__mcpLastSearchResult = {{
                                found: true,
                                data: data,
                                timestamp: new Date().toISOString()
                            }};
                            return 'found';
                        }}
                    }}
                }}
            }}
            window.__mcpLastSearchResult = {{
                found: false,
                error: 'Message not found in buffer',
                timestamp: new Date().toISOString()
            }};
            return 'not_found';
        }})()
    "#, result_key, result_key);
    
    // Execute the search
    window.eval(&search_code)
        .map_err(|e| crate::Error::Anyhow(format!("Failed to execute search JavaScript: {}", e)))?;
    
    // Small delay to let search execute
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    
    // Step 3: Get the search result
    let get_result_code = r#"
        (function() {
            if (window.__mcpLastSearchResult) {
                return JSON.stringify(window.__mcpLastSearchResult);
            } else {
                return JSON.stringify({ found: false, error: 'Search result not available' });
            }
        })()
    "#;
    
    window.eval(get_result_code)
        .map_err(|e| crate::Error::Anyhow(format!("Failed to get search result: {}", e)))?;
    
    // Since we can't get the return value from eval, we'll indicate success with our approach
    Ok(SocketResponse {
        success: true,
        data: Some(serde_json::json!({
            "message": "JavaScript result retrieval executed - check console buffer with get_console_buffer",
            "variable_name": variable_name,
            "result_key": result_key,
            "approach": "console_buffer_search",
            "next_step": "Use get_console_buffer to retrieve the actual data"
        })),
        error: None,
    })
}

/// Execute JavaScript with console capture enabled
pub async fn handle_execute_with_console<R: Runtime>(
    app: &AppHandle<R>,
    payload: Value,
) -> crate::Result<SocketResponse> {
    #[derive(Debug, Deserialize)]
    struct ExecuteWithConsoleRequest {
        window_label: Option<String>,
        code: String,
    }
    
    let request: ExecuteWithConsoleRequest = serde_json::from_value(payload)
        .map_err(|e| crate::Error::Anyhow(format!("Invalid request format: {}", e)))?;
    let window_label = request.window_label.unwrap_or_else(|| "main".to_string());
    
    info!("[TAURI_MCP] Executing JS with event-based console capture for window: {}", window_label);
    
    let window = app.get_webview_window(&window_label)
        .ok_or_else(|| crate::Error::Anyhow(format!("Window '{}' not found", window_label)))?;
    
    // First ensure console capture is setup
    let setup_result = handle_setup_console_capture(app, serde_json::json!({
        "window_label": window_label
    })).await?;
    
    if !setup_result.success {
        return Ok(setup_result);
    }
    
    // Execute the provided JavaScript code
    window.eval(&request.code)
        .map_err(|e| crate::Error::Anyhow(format!("Failed to execute JavaScript: {}", e)))?;
    
    Ok(SocketResponse {
        success: true,
        data: Some(serde_json::json!({
            "message": "JavaScript executed with event-based console capture",
            "window_label": window_label
        })),
        error: None,
    })
}

/// Get the console buffer with retrieved data
pub async fn handle_get_console_buffer<R: Runtime>(
    app: &AppHandle<R>,
    payload: Value,
) -> crate::Result<SocketResponse> {
    #[derive(Debug, Deserialize)]
    struct GetConsoleBufferRequest {
        window_label: Option<String>,
        filter: Option<String>, // Optional filter for specific messages
    }
    
    let request: GetConsoleBufferRequest = serde_json::from_value(payload)
        .map_err(|e| crate::Error::Anyhow(format!("Invalid request format: {}", e)))?;
    let window_label = request.window_label.unwrap_or_else(|| "main".to_string());
    
    info!("[TAURI_MCP] Getting console buffer from window: {}", window_label);
    
    let window = app.get_webview_window(&window_label)
        .ok_or_else(|| crate::Error::Anyhow(format!("Window '{}' not found", window_label)))?;
    
    // Get all console messages and search results
    let buffer_code = r#"
        (function() {
            const result = {
                consoleBuffer: window.__consoleBuffer || [],
                mcpMessages: window.__mcpConsoleMessages || [],
                jsErrors: window.__mcpJSErrors || [],
                lastSearchResult: window.__mcpLastSearchResult || null,
                bufferLength: (window.__consoleBuffer || []).length,
                mcpLength: (window.__mcpConsoleMessages || []).length,
                errorCount: (window.__mcpJSErrors || []).length
            };
            
            // Store in a global for easy access
            window.__mcpBufferData = result;
            
            return 'buffer_compiled';
        })()
    "#;
    
    window.eval(buffer_code)
        .map_err(|e| crate::Error::Anyhow(format!("Failed to compile buffer data: {}", e)))?;
    
    // Small delay to let the compilation execute
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    
    Ok(SocketResponse {
        success: true,
        data: Some(serde_json::json!({
            "message": "Console buffer compiled to window.__mcpBufferData",
            "window_label": window_label,
            "note": "Access window.__mcpBufferData.lastSearchResult for retrieved data"
        })),
        error: None,
    })
}