import { z } from "zod";
import { McpServer, Tool } from "@modelcontextprotocol/sdk/server.js";
import { socketClient } from "./client.js";

// Schema definitions for console operations
const ConsoleOutputArgsSchema = z.object({
  window_label: z.string().optional().describe("Window label to get console output from"),
  session_id: z.string().optional().describe("Console session ID for filtering"),
  clear_after_read: z.boolean().optional().describe("Whether to clear console buffer after reading"),
});

const SetupConsoleCaptureArgsSchema = z.object({
  window_label: z.string().optional().describe("Window label to setup console capture for"),
});

const ExecuteWithConsoleArgsSchema = z.object({
  window_label: z.string().optional().describe("Window label to execute JavaScript in"),
  code: z.string().describe("JavaScript code to execute"),
  capture_output: z.boolean().optional().describe("Whether to capture console output"),
});

/**
 * Register console capture tools with the MCP server
 */
export function registerConsoleCaptureTools(server: McpServer): void {
  // Tool 1: Get console output from the browser
  server.addTool({
    name: "get_console_output",
    description: "Retrieve console output (log, error, warn, info, debug) from the Tauri webview. This captures JavaScript console messages for debugging and monitoring purposes.",
    inputSchema: ConsoleOutputArgsSchema,
    handler: async (args) => {
      try {
        const result = await socketClient.sendCommand("get_console_output", args);
        
        if (result.success) {
          return {
            content: [
              {
                type: "text",
                text: `Console Output Retrieved:\n${JSON.stringify(result.data, null, 2)}`,
              },
            ],
          };
        } else {
          return {
            content: [
              {
                type: "text",
                text: `Failed to get console output: ${result.error || 'Unknown error'}`,
              },
            ],
            isError: true,
          };
        }
      } catch (error) {
        return {
          content: [
            {
              type: "text",
              text: `Error getting console output: ${error instanceof Error ? error.message : String(error)}`,
            },
          ],
          isError: true,
        };
      }
    },
  } as Tool);

  // Tool 2: Setup console capture system
  server.addTool({
    name: "setup_console_capture",
    description: "Initialize console capture system in the Tauri webview. This injects JavaScript code to intercept all console methods and capture their output for later retrieval.",
    inputSchema: SetupConsoleCaptureArgsSchema,
    handler: async (args) => {
      try {
        const result = await socketClient.sendCommand("setup_console_capture", args);
        
        if (result.success) {
          return {
            content: [
              {
                type: "text",
                text: `Console capture setup successful:\n${JSON.stringify(result.data, null, 2)}`,
              },
            ],
          };
        } else {
          return {
            content: [
              {
                type: "text",
                text: `Failed to setup console capture: ${result.error || 'Unknown error'}`,
              },
            ],
            isError: true,
          };
        }
      } catch (error) {
        return {
          content: [
            {
              type: "text",
              text: `Error setting up console capture: ${error instanceof Error ? error.message : String(error)}`,
            },
          ],
          isError: true,
        };
      }
    },
  } as Tool);

  // Tool 3: Execute JavaScript with console capture
  server.addTool({
    name: "execute_with_console",
    description: "Execute JavaScript code in the Tauri webview and automatically capture any console output generated during execution. This combines code execution with console monitoring for comprehensive debugging.",
    inputSchema: ExecuteWithConsoleArgsSchema,
    handler: async (args) => {
      try {
        const result = await socketClient.sendCommand("execute_with_console", args);
        
        if (result.success) {
          return {
            content: [
              {
                type: "text",
                text: `JavaScript executed with console capture:\n${JSON.stringify(result.data, null, 2)}`,
              },
            ],
          };
        } else {
          return {
            content: [
              {
                type: "text",
                text: `Failed to execute JavaScript with console capture: ${result.error || 'Unknown error'}`,
              },
            ],
            isError: true,
          };
        }
      } catch (error) {
        return {
          content: [
            {
              type: "text",
              text: `Error executing JavaScript with console capture: ${error instanceof Error ? error.message : String(error)}`,
            },
          ],
          isError: true,
        };
      }
    },
  } as Tool);

  console.log("Console capture tools registered successfully");
}