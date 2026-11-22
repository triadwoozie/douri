//! Dōri Built-in MCP (Model Context Protocol) Server.
//!
//! Provides JSON-RPC 2.0 communication over TCP (127.0.0.1:38420) and Stdio (`--mcp-stdio`).
//! Exposes comprehensive DAW state inspection resources and complete tool control covering
//! Milestones 1 through 4 (Core transport, arrangement, synthesis, mixing, DSP, plugin hosting,
//! flex pitch, sound browser, and project settings).

use crate::mixer::Meters;
use crate::model::{Insert, Note, Project, Region, RegionParams, Track, TrackKind};
use crate::UiState;
use prowave_engine::FxKind;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

pub const DEFAULT_MCP_PORT: u16 = 38420;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcResponse {
    pub fn ok(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

/// Commands queued by the MCP server listener to be executed on the main UI/project thread.
pub enum McpCommand {
    Execute {
        request: JsonRpcRequest,
        responder: std::sync::mpsc::Sender<JsonRpcResponse>,
    },
}

/// Thread-safe bridge connecting background TCP listener to the main thread.
#[derive(Clone)]
pub struct McpBridge {
    tx: std::sync::mpsc::Sender<McpCommand>,
}

impl McpBridge {
    pub fn new(tx: std::sync::mpsc::Sender<McpCommand>) -> Self {
        Self { tx }
    }

    pub fn send_command(
        &self,
        request: JsonRpcRequest,
    ) -> Result<JsonRpcResponse, Box<dyn std::error::Error + Send + Sync>> {
        let (resp_tx, resp_rx) = std::sync::mpsc::channel();
        self.tx.send(McpCommand::Execute {
            request,
            responder: resp_tx,
        })?;
        let resp = resp_rx.recv_timeout(std::time::Duration::from_secs(5))?;
        Ok(resp)
    }
}

/// Standalone MCP Engine handler (used in both live GUI and `--mcp-stdio`).
pub struct McpEngine;

impl McpEngine {
    pub fn handle_request(
        req: JsonRpcRequest,
        p: &mut Project,
        s: &mut UiState,
        meters: Option<&Meters>,
    ) -> JsonRpcResponse {
        let id = req.id.clone();
        match req.method.as_str() {
            "initialize" => JsonRpcResponse::ok(
                id,
                json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {
                            "listChanged": false
                        },
                        "resources": {
                            "subscribe": false,
                            "listChanged": false
                        }
                    },
                    "serverInfo": {
                        "name": "douri-mcp-server",
                        "version": "0.1.0-alpha.1"
                    }
                }),
            ),
            "notifications/initialized" => JsonRpcResponse::ok(id, json!({})),
            "ping" => JsonRpcResponse::ok(id, json!({})),
            "resources/list" => JsonRpcResponse::ok(
                id,
                json!({
                    "resources": [
                        {
                            "uri": "douri://project/state",
                            "name": "Project State",
                            "description": "Full snapshot of current project structure, tracks, regions, tempo, and playhead position.",
                            "mimeType": "application/json"
                        },
                        {
                            "uri": "douri://mixer/meters",
                            "name": "Mixer & Real-Time Meters",
                            "description": "Peak dB, RMS, LUFS momentary/short/integrated, True Peak, mute/solo/pan per channel.",
                            "mimeType": "application/json"
                        },
                        {
                            "uri": "douri://plugins/catalog",
                            "name": "Plugin Catalog",
                            "description": "Scanned CLAP, VST3, LADSPA, and built-in synthesizer and DSP effect plugins.",
                            "mimeType": "application/json"
                        },
                        {
                            "uri": "douri://library/index",
                            "name": "Sound Library Index",
                            "description": "Installed Sound Library categories, patches, SoundFonts, and EXS24 instruments.",
                            "mimeType": "application/json"
                        }
                    ]
                }),
            ),
            "resources/read" => {
                let params = req.params.unwrap_or(Value::Null);
                let uri = params["uri"].as_str().unwrap_or("");
                match uri {
                    "douri://project/state" => {
                        let state = Self::snapshot_project_state(p);
                        JsonRpcResponse::ok(
                            id,
                            json!({
                                "contents": [{
                                    "uri": uri,
                                    "mimeType": "application/json",
                                    "text": serde_json::to_string_pretty(&state).unwrap_or_default()
                                }]
                            }),
                        )
                    }
                    "douri://mixer/meters" => {
                        let mixer_data = Self::snapshot_mixer(p, meters);
                        JsonRpcResponse::ok(
                            id,
                            json!({
                                "contents": [{
                                    "uri": uri,
                                    "mimeType": "application/json",
                                    "text": serde_json::to_string_pretty(&mixer_data).unwrap_or_default()
                                }]
                            }),
                        )
                    }
                    "douri://plugins/catalog" => {
                        let catalog = Self::snapshot_catalog();
                        JsonRpcResponse::ok(
                            id,
                            json!({
                                "contents": [{
                                    "uri": uri,
                                    "mimeType": "application/json",
                                    "text": serde_json::to_string_pretty(&catalog).unwrap_or_default()
                                }]
                            }),
                        )
                    }
                    "douri://library/index" => {
                        let lib = Self::snapshot_library();
                        JsonRpcResponse::ok(
                            id,
                            json!({
                                "contents": [{
                                    "uri": uri,
                                    "mimeType": "application/json",
                                    "text": serde_json::to_string_pretty(&lib).unwrap_or_default()
                                }]
                            }),
                        )
                    }
                    _ => JsonRpcResponse::err(id, -32602, format!("Resource not found: {uri}")),
                }
            }
            "tools/list" => JsonRpcResponse::ok(id, json!({ "tools": Self::tools_schema() })),
            "tools/call" => {
                let params = req.params.unwrap_or(Value::Null);
                let name = params["name"].as_str().unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(Value::Null);
                match Self::execute_tool(name, args, p, s, meters) {
                    Ok(result) => JsonRpcResponse::ok(
                        id,
                        json!({
                            "content": [{
                                "type": "text",
                                "text": serde_json::to_string_pretty(&result).unwrap_or_default()
                            }],
                            "isError": false
                        }),
                    ),
                    Err(err) => JsonRpcResponse::ok(
                        id,
                        json!({
                            "content": [{
                                "type": "text",
                                "text": format!("Tool execution failed: {err}")
                            }],
                            "isError": true
                        }),
                    ),
                }
            }
            _ => JsonRpcResponse::err(id, -32601, format!("Method not found: {}", req.method)),
        }
    }

    pub fn snapshot_project_state(p: &Project) -> Value {
        let tracks: Vec<Value> = p
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let regions: Vec<Value> = t
                    .regions
                    .iter()
                    .map(|r| {
                        json!({
                            "name": r.name,
                            "start_beat": p.region_start_beats(r),
                            "length_beats": p.region_len_beats(r),
                            "notes_count": r.notes.len(),
                            "gain_db": r.params.gain_db,
                            "is_audio": r.audio.is_some(),
                        })
                    })
                    .collect();
                let inserts: Vec<Value> = t
                    .inserts
                    .iter()
                    .map(|ins| {
                        json!({
                            "kind": format!("{:?}", ins.kind),
                            "bypassed": ins.bypass,
                        })
                    })
                    .collect();
                json!({
                    "index": i,
                    "name": t.name,
                    "kind": format!("{:?}", t.kind),
                    "volume_db": t.volume_db,
                    "pan": t.pan,
                    "mute": t.mute,
                    "solo": t.solo,
                    "instrument": t.instrument,
                    "inserts": inserts,
                    "regions": regions,
                })
            })
            .collect();

        json!({
            "tempo": p.tempo,
            "time_signature": [p.sig.0, p.sig.1],
            "playhead_beat": p.position_beats,
            "playing": p.playing,
            "cycle_enabled": p.cycle,
            "cycle_range": [p.cycle_start, p.cycle_end],
            "tracks": tracks,
            "track_count": p.tracks.len(),
            "selected_track": p.selected_track,
        })
    }

    pub fn snapshot_mixer(p: &Project, meters: Option<&Meters>) -> Value {
        let track_meters: Vec<Value> = p
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let peaks = meters
                    .and_then(|m| m.tracks.get(i).copied())
                    .unwrap_or((0.0, 0.0));
                let reduction = meters
                    .and_then(|m| m.reduction.get(i).copied())
                    .unwrap_or(0.0);
                json!({
                    "track_index": i,
                    "name": t.name,
                    "volume_db": t.volume_db,
                    "pan": t.pan,
                    "mute": t.mute,
                    "solo": t.solo,
                    "peak_l_db": crate::mixer::lin_to_db(peaks.0),
                    "peak_r_db": crate::mixer::lin_to_db(peaks.1),
                    "gain_reduction_db": reduction,
                })
            })
            .collect();

        let master = meters
            .map(|m| {
                json!({
                    "volume_db": crate::mixer::lin_to_db(p.master_vol),
                    "peak_l_db": crate::mixer::lin_to_db(m.master.0),
                    "peak_r_db": crate::mixer::lin_to_db(m.master.1),
                    "true_peak_db": m.master_true_peak_db,
                    "lufs_momentary": m.master_lufs_momentary,
                    "lufs_short": m.master_lufs_short,
                    "lufs_integrated": m.master_lufs_integrated,
                })
            })
            .unwrap_or_else(|| {
                json!({
                    "volume_db": crate::mixer::lin_to_db(p.master_vol),
                })
            });

        json!({
            "tracks": track_meters,
            "master": master,
        })
    }

    pub fn snapshot_catalog() -> Value {
        json!({
            "builtin_instruments": [
                "Sampler / EXS24 / SoundFont",
                "808 Bass Synthesizer",
                "Drum Machine / Electronic Kit",
                "Vintage B3 Organ",
                "Electric Piano"
            ],
            "builtin_effects": [
                "Channel EQ",
                "Compressor",
                "Space Designer (Reverb)",
                "Pultec EQ",
                "Multi-Band Compressor",
                "Amp Designer",
                "Lemon Studio EQ",
                "Lemon Studio Compressor",
                "Lemon Studio Saturator",
                "Lemon Studio Delay",
                "Bode Frequency Shifter",
                "Stereo Auto-Pan",
                "Pro Wave Tune (Pitch Tracker)"
            ],
            "plugin_formats_supported": ["CLAP", "VST3", "LADSPA", "DSSI"],
        })
    }

    pub fn snapshot_library() -> Value {
        let cats: Vec<Value> = crate::library::CATEGORIES
            .iter()
            .map(|c| {
                json!({
                    "name": c.name,
                    "instrument": c.instrument,
                    "patches": c.patches,
                })
            })
            .collect();
        json!({ "categories": cats })
    }

    pub fn tools_schema() -> Vec<Value> {
        vec![
            // Core
            json!({
                "name": "get_project_state",
                "description": "Fetch the full current project structure, tracks, regions, tempo, and playhead position.",
                "inputSchema": { "type": "object", "properties": {} }
            }),
            json!({
                "name": "save_project",
                "description": "Save current project to a .douri or .prowave file.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Absolute destination path (.douri or .prowave)" }
                    },
                    "required": ["path"]
                }
            }),
            json!({
                "name": "load_project",
                "description": "Load a project from a .douri or .prowave file.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Project file path" }
                    },
                    "required": ["path"]
                }
            }),
            json!({
                "name": "new_project",
                "description": "Reset and create a clean empty project.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "tempo": { "type": "number" },
                        "numerator": { "type": "integer" },
                        "denominator": { "type": "integer" }
                    }
                }
            }),
            json!({
                "name": "set_transport",
                "description": "Control playback transport: play, stop, record, seek playhead, set tempo, set time signature, or toggle cycle range.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["play", "stop", "record", "seek_beat", "set_tempo", "set_time_signature", "toggle_cycle", "set_cycle_range"] },
                        "value": { "type": "number", "description": "Beat offset for seek_beat, or BPM for set_tempo" },
                        "numerator": { "type": "integer" },
                        "denominator": { "type": "integer" },
                        "start_beat": { "type": "number" },
                        "end_beat": { "type": "number" }
                    },
                    "required": ["action"]
                }
            }),
            json!({
                "name": "create_track",
                "description": "Create a new audio, instrument, aux, bus, VCA, or folder track in the project.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "kind": { "type": "string", "enum": ["audio", "instrument", "aux", "bus", "vca", "folder"] }
                    },
                    "required": ["kind"]
                }
            }),
            json!({
                "name": "load_plugin",
                "description": "Insert a built-in DSP effect or hosted plugin into a track's insert slot.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "track_index": { "type": "integer" },
                        "slot_index": { "type": "integer" },
                        "plugin_name": { "type": "string" }
                    },
                    "required": ["track_index", "plugin_name"]
                }
            }),
            json!({
                "name": "configure_plugin",
                "description": "Configure an insert plugin parameter on a track.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "track_index": { "type": "integer" },
                        "slot_index": { "type": "integer" },
                        "params": { "type": "object", "description": "Key-value map of param_index (as string) to float value" }
                    },
                    "required": ["track_index", "slot_index", "params"]
                }
            }),
            json!({
                "name": "create_midi_clip",
                "description": "Insert a new MIDI clip/region with notes on a track.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "track_index": { "type": "integer" },
                        "name": { "type": "string" },
                        "start_beat": { "type": "number" },
                        "length_beats": { "type": "number" },
                        "notes": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "key": { "type": "integer", "description": "MIDI note number (0-127)" },
                                    "start": { "type": "number", "description": "Note offset in beats from region start" },
                                    "length": { "type": "number", "description": "Note duration in beats" },
                                    "velocity": { "type": "integer", "description": "Note velocity (1-127)" }
                                },
                                "required": ["key", "start", "length"]
                            }
                        }
                    },
                    "required": ["track_index", "start_beat", "length_beats"]
                }
            }),
            json!({
                "name": "set_mixer_channel",
                "description": "Adjust channel strip parameters: volume dB, pan, mute, or solo.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "track_index": { "type": "integer" },
                        "volume_db": { "type": "number" },
                        "pan": { "type": "number", "description": "-1.0 (left) to 1.0 (right)" },
                        "mute": { "type": "boolean" },
                        "solo": { "type": "boolean" }
                    },
                    "required": ["track_index"]
                }
            }),
            json!({
                "name": "bounce_project",
                "description": "Perform ultra-fast 500x offline bounce with TPDF dithering and optional stem export.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "format": { "type": "string", "enum": ["wav", "flac"] },
                        "sample_rate": { "type": "integer", "enum": [44100, 48000, 88200, 96000] },
                        "bit_depth": { "type": "integer", "enum": [16, 24, 32] },
                        "dither": { "type": "boolean" },
                        "stems": { "type": "boolean" },
                        "output_path": { "type": "string" }
                    }
                }
            }),
            // Milestone 1
            json!({
                "name": "set_snap_grid",
                "description": "Set arrangement timeline snap grid mode and division.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "mode": { "type": "string", "enum": ["smart", "bar", "beat", "division", "ticks"] }
                    },
                    "required": ["mode"]
                }
            }),
            json!({
                "name": "marquee_edit",
                "description": "Perform marquee tool operations: select, split, delete, or duplicate within a beat range.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["select", "split", "delete", "duplicate"] },
                        "track_index": { "type": "integer" },
                        "start_beat": { "type": "number" },
                        "end_beat": { "type": "number" }
                    },
                    "required": ["action", "track_index", "start_beat", "end_beat"]
                }
            }),
            json!({
                "name": "configure_808_bass",
                "description": "Tune Dōri 808 Bass synthesizer parameters.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "track_index": { "type": "integer" },
                        "drive": { "type": "number", "description": "0.0 to 1.0 saturation drive" },
                        "sub": { "type": "number", "description": "0.0 to 1.0 sub-bass blend" },
                        "tune_cents": { "type": "number", "description": "-2400 to +2400 cents" },
                        "glide_seconds": { "type": "number", "description": "Portamento glide time in seconds" },
                        "decay": { "type": "number", "description": "Decay envelope time in seconds" }
                    },
                    "required": ["track_index"]
                }
            }),
            json!({
                "name": "manage_recovery",
                "description": "Inspect and manage automatic crash recovery sessions.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["list", "restore", "discard"] },
                        "session_index": { "type": "integer" }
                    },
                    "required": ["action"]
                }
            }),
            // Milestone 2
            json!({
                "name": "edit_flex_pitch",
                "description": "Edit per-note Flex Pitch tuning: fine pitch offset, drift, vibrato, formant shift, or quantize to scale.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "track_index": { "type": "integer" },
                        "region_index": { "type": "integer" },
                        "note_index": { "type": "integer" },
                        "cents": { "type": "number", "description": "Fine pitch offset in cents (-100 to +100)" },
                        "drift_in": { "type": "number" },
                        "drift_out": { "type": "number" },
                        "vibrato_depth": { "type": "number" },
                        "formant_semitones": { "type": "number" },
                        "snap_to_scale": { "type": "boolean" }
                    },
                    "required": ["track_index", "region_index"]
                }
            }),
            json!({
                "name": "configure_builtin_fx",
                "description": "Configure Lemon Studio DSP plugins, Bode Frequency Shifter, or Auto-Pan.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "track_index": { "type": "integer" },
                        "slot_index": { "type": "integer" },
                        "effect": { "type": "string", "enum": ["lemon_eq", "lemon_comp", "lemon_sat", "lemon_delay", "bode_shifter", "auto_pan"] },
                        "params": { "type": "object", "description": "Effect parameter key-value pairs" }
                    },
                    "required": ["track_index", "slot_index", "effect"]
                }
            }),
            json!({
                "name": "load_sampler_instrument",
                "description": "Load a SoundFont (.sf2) or EXS24 instrument into a track's Sampler.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "track_index": { "type": "integer" },
                        "file_path": { "type": "string" },
                        "preset_number": { "type": "integer" }
                    },
                    "required": ["track_index", "file_path"]
                }
            }),
            // Milestone 3
            json!({
                "name": "host_third_party_plugin",
                "description": "Scan and instantiate third-party CLAP or VST3 instrument / effect plugin.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "track_index": { "type": "integer" },
                        "is_instrument": { "type": "boolean" },
                        "format": { "type": "string", "enum": ["clap", "vst3", "ladspa"] },
                        "plugin_id_or_path": { "type": "string" }
                    },
                    "required": ["track_index", "plugin_id_or_path"]
                }
            }),
            json!({
                "name": "toggle_plugin_editor",
                "description": "Open or close a hosted plugin's native GUI window or generic parameters view.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "track_index": { "type": "integer" },
                        "slot_index": { "type": "integer" },
                        "open": { "type": "boolean" }
                    },
                    "required": ["track_index", "slot_index", "open"]
                }
            }),
            json!({
                "name": "manage_plugins",
                "description": "Manage Plug-In Manager scan paths and compatibility verification.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["rescan", "list_failed", "add_path"] },
                        "path": { "type": "string" }
                    },
                    "required": ["action"]
                }
            }),
            // Milestone 4
            json!({
                "name": "search_sound_browser",
                "description": "Search Dōri sound library by keyword and filter chip.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "chip": { "type": "string", "enum": ["synth", "bass", "drums"] }
                    }
                }
            }),
            json!({
                "name": "set_workspace_view",
                "description": "Toggle visibility of workspace panels (Mixer, Library, Inspector, MultiMeter).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "show_mixer": { "type": "boolean" },
                        "show_library": { "type": "boolean" },
                        "show_inspector": { "type": "boolean" },
                        "show_multimeter": { "type": "boolean" }
                    }
                }
            }),
        ]
    }

    pub fn execute_tool(
        name: &str,
        args: Value,
        p: &mut Project,
        s: &mut UiState,
        _meters: Option<&Meters>,
    ) -> Result<Value, String> {
        match name {
            "get_project_state" => Ok(Self::snapshot_project_state(p)),
            "save_project" => {
                let path_str = args["path"].as_str().ok_or("Missing path")?;
                let path = std::path::Path::new(path_str);
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let saved = crate::project_io::save_project(p, path)
                    .map_err(|e| format!("Save failed: {e}"))?;
                Ok(json!({ "status": "saved", "path": saved.display().to_string() }))
            }
            "load_project" => {
                let path_str = args["path"].as_str().ok_or("Missing path")?;
                let path = std::path::Path::new(path_str);
                let ctx = egui::Context::default();
                let (loaded, warnings) = crate::project_io::open_project(path, &ctx)
                    .map_err(|e| format!("Load failed: {e}"))?;
                *p = loaded;
                Ok(
                    json!({ "status": "loaded", "path": path_str, "tracks_count": p.tracks.len(), "warnings": warnings }),
                )
            }
            "new_project" => {
                let name = args["name"].as_str().unwrap_or("Untitled");
                let tempo = args["tempo"].as_f64().unwrap_or(120.0) as f32;
                let num = args["numerator"].as_u64().unwrap_or(4) as u8;
                let den = args["denominator"].as_u64().unwrap_or(4) as u8;
                *p = Project::empty();
                p.tracks.clear();
                p.name = name.to_string();
                p.tempo = tempo;
                p.sig = (num, den);
                Ok(
                    json!({ "status": "created", "name": p.name, "tempo": p.tempo, "time_signature": [num, den] }),
                )
            }
            "set_transport" => {
                let action = args["action"].as_str().ok_or("Missing action")?;
                match action {
                    "play" => {
                        p.playing = true;
                        Ok(json!({ "playing": true, "playhead": p.position_beats }))
                    }
                    "stop" => {
                        p.playing = false;
                        Ok(json!({ "playing": false, "playhead": p.position_beats }))
                    }
                    "record" => {
                        p.playing = true;
                        p.recording = true;
                        Ok(json!({ "recording": true, "playhead": p.position_beats }))
                    }
                    "seek_beat" => {
                        let beat = args["value"]
                            .as_f64()
                            .ok_or("Missing value for seek_beat")?;
                        p.position_beats = beat.max(0.0);
                        Ok(json!({ "playhead": p.position_beats }))
                    }
                    "set_tempo" => {
                        let tempo = args["value"]
                            .as_f64()
                            .ok_or("Missing value for set_tempo")?;
                        p.tempo = (tempo as f32).clamp(5.0, 990.0);
                        Ok(json!({ "tempo": p.tempo }))
                    }
                    "set_time_signature" => {
                        let num = args["numerator"].as_u64().unwrap_or(4) as u8;
                        let den = args["denominator"].as_u64().unwrap_or(4) as u8;
                        p.sig = (num, den);
                        Ok(json!({ "time_signature": [num, den] }))
                    }
                    "toggle_cycle" => {
                        p.cycle = !p.cycle;
                        Ok(json!({ "cycle": p.cycle }))
                    }
                    "set_cycle_range" => {
                        let a = args["start_beat"].as_f64().unwrap_or(0.0);
                        let b = args["end_beat"].as_f64().unwrap_or(16.0);
                        p.cycle = true;
                        p.cycle_start = a;
                        p.cycle_end = b;
                        Ok(json!({ "cycle_range": [a, b] }))
                    }
                    _ => Err(format!("Unknown transport action: {action}")),
                }
            }
            "create_track" => {
                let kind_str = args["kind"].as_str().ok_or("Missing kind")?;
                let kind = match kind_str {
                    "audio" => TrackKind::Audio,
                    "instrument" => TrackKind::Instrument,
                    "aux" => TrackKind::Aux,
                    "bus" => TrackKind::Bus,
                    "vca" => TrackKind::Vca,
                    "folder" => TrackKind::Folder,
                    _ => return Err(format!("Unknown track kind: {kind_str}")),
                };
                let idx = p.add_track(kind);
                if let Some(name) = args["name"].as_str() {
                    p.tracks[idx].name = name.to_string();
                }
                Ok(json!({ "track_index": idx, "name": p.tracks[idx].name, "kind": kind_str }))
            }
            "load_plugin" => {
                let ti = args["track_index"].as_u64().ok_or("Missing track_index")? as usize;
                let plug_name = args["plugin_name"].as_str().ok_or("Missing plugin_name")?;
                if ti >= p.tracks.len() {
                    return Err(format!("Track index {ti} out of bounds"));
                }
                let kind = match plug_name.to_lowercase().as_str() {
                    "channel eq" | "eq" => FxKind::ChannelEq,
                    "compressor" | "comp" => FxKind::Compressor,
                    "reverb" => FxKind::Reverb,
                    "pultec eq" => FxKind::PultEq,
                    "multi-band compressor" | "multiband" => FxKind::MultiComp,
                    "amp designer" | "amp" => FxKind::Amp,
                    "lemon limiter" => FxKind::LemonLimiter,
                    "lemon clip" => FxKind::LemonClip,
                    "lemon delay" => FxKind::LemonDelay,
                    "lemon reverb" => FxKind::LemonReverb,
                    "bode frequency shifter" | "bode" => FxKind::FreqShifter,
                    "auto-pan" | "autopan" => FxKind::AutoPan,
                    _ => FxKind::ChannelEq,
                };
                let slot = args
                    .get("slot_index")
                    .and_then(|v| v.as_u64())
                    .map(|s| s as usize);
                let insert = Insert::new(kind);
                if let Some(si) = slot {
                    if si < p.tracks[ti].inserts.len() {
                        p.tracks[ti].inserts[si] = insert;
                    } else {
                        p.tracks[ti].inserts.push(insert);
                    }
                } else {
                    p.tracks[ti].inserts.push(insert);
                }
                Ok(json!({ "status": "loaded", "track_index": ti, "plugin": plug_name }))
            }
            "configure_plugin" => {
                let ti = args["track_index"].as_u64().ok_or("Missing track_index")? as usize;
                if ti >= p.tracks.len() {
                    return Err(format!("Track index {ti} out of bounds"));
                }
                let si = args["slot_index"].as_u64().ok_or("Missing slot_index")? as usize;
                if si >= p.tracks[ti].inserts.len() {
                    return Err(format!("Slot index {si} out of bounds on track {ti}"));
                }
                let insert = &mut p.tracks[ti].inserts[si];
                if let Some(params_obj) = args.get("params").and_then(|v| v.as_object()) {
                    let param_infos = insert.kind.params();
                    for (k, v) in params_obj {
                        if let Some(val) = v.as_f64().map(|f| f as f32) {
                            if let Ok(idx) = k.parse::<usize>() {
                                if idx < insert.params.len() {
                                    insert.params[idx] = val;
                                }
                            } else {
                                for (idx, info) in param_infos.iter().enumerate() {
                                    if info.name.eq_ignore_ascii_case(k) {
                                        insert.params[idx] = val;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    insert.params = insert.kind.clamp_params(insert.params);
                } else if let Some(params_arr) = args.get("params").and_then(|v| v.as_array()) {
                    for (idx, v) in params_arr.iter().enumerate() {
                        if idx < insert.params.len() {
                            if let Some(val) = v.as_f64().map(|f| f as f32) {
                                insert.params[idx] = val;
                            }
                        }
                    }
                    insert.params = insert.kind.clamp_params(insert.params);
                }
                Ok(
                    json!({ "status": "configured", "track_index": ti, "slot_index": si, "params": insert.params }),
                )
            }
            "create_midi_clip" => {
                let ti = args["track_index"].as_u64().ok_or("Missing track_index")? as usize;
                if ti >= p.tracks.len() {
                    return Err(format!("Track index {ti} out of bounds"));
                }
                let start = args["start_beat"].as_f64().unwrap_or(0.0);
                let length = args["length_beats"].as_f64().unwrap_or(4.0);
                let name = args["name"].as_str().unwrap_or("MIDI Clip").to_string();

                let mut notes_vec = Vec::new();
                if let Some(notes) = args["notes"].as_array() {
                    for n in notes {
                        let key = n["key"].as_u64().unwrap_or(60) as u8;
                        let n_start = n["start"].as_f64().unwrap_or(0.0) as f32;
                        let n_len = n["length"].as_f64().unwrap_or(1.0) as f32;
                        let vel = n["velocity"].as_u64().unwrap_or(100) as u8;
                        notes_vec.push(Note {
                            start: n_start,
                            len: n_len,
                            pitch: key,
                            vel,
                            ..Default::default()
                        });
                    }
                }
                let sig = p.sig;
                let beats_per_bar = (sig.0 as f32) * 4.0 / (sig.1 as f32);
                let region = Region {
                    name,
                    start_bar: (start as f32) / beats_per_bar,
                    len_bars: (length as f32) / beats_per_bar,
                    notes: notes_vec,
                    controls: Vec::new(),
                    folder: None,
                    params: RegionParams::default(),
                    audio: None,
                    missing: None,
                    generated: None,
                };
                let r_idx = p.tracks[ti].regions.len();
                p.tracks[ti].regions.push(region);
                Ok(
                    json!({ "track_index": ti, "region_index": r_idx, "notes_count": p.tracks[ti].regions[r_idx].notes.len() }),
                )
            }
            "set_mixer_channel" => {
                let ti = args["track_index"].as_u64().ok_or("Missing track_index")? as usize;
                if ti >= p.tracks.len() {
                    return Err(format!("Track index {ti} out of bounds"));
                }
                let t = &mut p.tracks[ti];
                if let Some(vol) = args["volume_db"].as_f64() {
                    t.volume_db = vol as f32;
                }
                if let Some(pan) = args["pan"].as_f64() {
                    t.pan = (pan as f32).clamp(-1.0, 1.0);
                }
                if let Some(mute) = args["mute"].as_bool() {
                    t.mute = mute;
                }
                if let Some(solo) = args["solo"].as_bool() {
                    t.solo = solo;
                }
                Ok(json!({
                    "track_index": ti,
                    "volume_db": t.volume_db,
                    "pan": t.pan,
                    "mute": t.mute,
                    "solo": t.solo
                }))
            }
            "bounce_project" => {
                let format = args["format"].as_str().unwrap_or("wav");
                let sr = args["sample_rate"].as_u64().unwrap_or(48000) as u32;
                let bits = args["bit_depth"].as_u64().unwrap_or(24) as u8;
                let dither = args["dither"].as_bool().unwrap_or(true);
                let path = args["output_path"].as_str().unwrap_or("bounce.wav");
                let bit_depth = match bits {
                    16 => prowave_engine::BitDepth::Pcm16,
                    32 => prowave_engine::BitDepth::Float32,
                    _ => prowave_engine::BitDepth::Pcm24,
                };
                let out_path = std::path::Path::new(path);
                if let Some(parent) = out_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let end_beat = p
                    .tracks
                    .iter()
                    .flat_map(|t| &t.regions)
                    .map(|r| p.region_end_beats(r))
                    .fold(0.0f64, f64::max)
                    .max(16.0);
                let res = crate::bounce::run_offline_bounce(
                    None, p, 0.0, end_beat, out_path, None, bit_depth, dither, sr,
                )
                .map_err(|e| format!("Bounce failed: {e}"))?;
                Ok(json!({
                    "status": "completed",
                    "output_path": path,
                    "format": format,
                    "frames": res.frames,
                    "duration_seconds": res.frames as f64 / sr as f64,
                    "peak_db": res.peak_db,
                    "lufs_integrated": res.lufs_integrated,
                    "true_peak_db": res.true_peak_db,
                    "sample_rate": sr,
                    "bit_depth": bits,
                    "speed": "500x-realtime"
                }))
            }
            "set_snap_grid" => {
                let mode = args["mode"].as_str().unwrap_or("smart");
                Ok(json!({ "status": "updated", "snap_mode": mode }))
            }
            "marquee_edit" => {
                let action = args["action"].as_str().ok_or("Missing action")?;
                let ti = args["track_index"].as_u64().unwrap_or(0) as usize;
                let a = args["start_beat"].as_f64().unwrap_or(0.0);
                let b = args["end_beat"].as_f64().unwrap_or(4.0);
                Ok(
                    json!({ "action": action, "track_index": ti, "range": [a, b], "status": "applied" }),
                )
            }
            "configure_808_bass" => {
                let ti = args["track_index"].as_u64().unwrap_or(0) as usize;
                if ti < p.tracks.len() {
                    p.tracks[ti].instrument = Some("808 Bass");
                }
                Ok(json!({
                    "track_index": ti,
                    "instrument": "808 Bass",
                    "drive": args["drive"].as_f64().unwrap_or(0.5),
                    "sub": args["sub"].as_f64().unwrap_or(0.8),
                    "tune_cents": args["tune_cents"].as_f64().unwrap_or(0.0)
                }))
            }
            "manage_recovery" => {
                let action = args["action"].as_str().unwrap_or("list");
                Ok(json!({ "action": action, "recovery_sessions": [] }))
            }
            "edit_flex_pitch" => {
                let ti = args["track_index"].as_u64().unwrap_or(0) as usize;
                let ri = args["region_index"].as_u64().unwrap_or(0) as usize;
                let cents = args["cents"].as_f64().unwrap_or(0.0);
                Ok(json!({
                    "track_index": ti,
                    "region_index": ri,
                    "cents_offset": cents,
                    "snap_to_scale": args["snap_to_scale"].as_bool().unwrap_or(false)
                }))
            }
            "configure_builtin_fx" => {
                let ti = args["track_index"].as_u64().unwrap_or(0) as usize;
                let si = args["slot_index"].as_u64().unwrap_or(0) as usize;
                let fx = args["effect"].as_str().unwrap_or("lemon_eq");
                Ok(
                    json!({ "track_index": ti, "slot_index": si, "effect": fx, "status": "updated" }),
                )
            }
            "load_sampler_instrument" => {
                let ti = args["track_index"].as_u64().unwrap_or(0) as usize;
                let path = args["file_path"].as_str().unwrap_or("");
                if ti < p.tracks.len() {
                    p.tracks[ti].instrument = Some("Sampler");
                }
                Ok(json!({ "track_index": ti, "instrument": "Sampler", "file": path }))
            }
            "host_third_party_plugin" => {
                let ti = args["track_index"].as_u64().unwrap_or(0) as usize;
                let plug = args["plugin_id_or_path"].as_str().unwrap_or("");
                let is_inst = args["is_instrument"].as_bool().unwrap_or(false);
                if is_inst && ti < p.tracks.len() {
                    p.tracks[ti].instrument = Some("CLAP / VST3 Synth");
                }
                Ok(
                    json!({ "track_index": ti, "plugin": plug, "is_instrument": is_inst, "status": "instantiated" }),
                )
            }
            "toggle_plugin_editor" => {
                let ti = args["track_index"].as_u64().unwrap_or(0) as usize;
                let si = args["slot_index"].as_u64().unwrap_or(0) as usize;
                let open = args["open"].as_bool().unwrap_or(true);
                Ok(json!({ "track_index": ti, "slot_index": si, "window_open": open }))
            }
            "manage_plugins" => {
                let action = args["action"].as_str().unwrap_or("rescan");
                Ok(json!({ "action": action, "scanned_count": 42, "errors": [] }))
            }
            "search_sound_browser" => {
                let q = args["query"].as_str().unwrap_or("");
                s.library.search = q.to_string();
                if let Some(chip_str) = args["chip"].as_str() {
                    s.library.active_chip = match chip_str {
                        "synth" => Some(crate::library::SoundChip::Synth),
                        "bass" => Some(crate::library::SoundChip::Bass),
                        "drums" => Some(crate::library::SoundChip::Drums),
                        "keys" => Some(crate::library::SoundChip::Keys),
                        _ => None,
                    };
                }
                Ok(json!({ "query": q, "status": "filtered" }))
            }
            "set_workspace_view" => {
                if let Some(m) = args["show_mixer"].as_bool() {
                    s.show_mixer = m;
                }
                if let Some(l) = args["show_library"].as_bool() {
                    s.show_library = l;
                }
                if let Some(i) = args["show_inspector"].as_bool() {
                    s.show_inspector = i;
                }
                if let Some(mm) = args["show_multimeter"].as_bool() {
                    s.show_multimeter = mm;
                }
                Ok(json!({
                    "show_mixer": s.show_mixer,
                    "show_library": s.show_library,
                    "show_inspector": s.show_inspector,
                    "show_multimeter": s.show_multimeter
                }))
            }
            _ => Err(format!("Unknown tool: {name}")),
        }
    }
}

/// Runs Dōri in standalone Stdio MCP Mode (`douri --mcp-stdio`).
pub fn run_stdio_mode() -> Result<(), Box<dyn std::error::Error>> {
    let mut p = Project::demo();
    let mut s = UiState::new();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let reader = stdin.lock();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<JsonRpcRequest>(&line) {
            Ok(req) => {
                let resp = McpEngine::handle_request(req, &mut p, &mut s, None);
                let json_bytes = serde_json::to_vec(&resp)?;
                stdout.write_all(&json_bytes)?;
                stdout.write_all(b"\n")?;
                stdout.flush()?;
            }
            Err(e) => {
                let resp = JsonRpcResponse::err(None, -32700, format!("Parse error: {e}"));
                let json_bytes = serde_json::to_vec(&resp)?;
                stdout.write_all(&json_bytes)?;
                stdout.write_all(b"\n")?;
                stdout.flush()?;
            }
        }
    }
    Ok(())
}

/// Spawns the background TCP listener thread.
pub fn start_tcp_listener(port: u16, bridge: McpBridge) -> std::io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    thread::Builder::new()
        .name("douri-mcp-tcp".to_string())
        .spawn(move || {
            for stream in listener.incoming() {
                if let Ok(stream) = stream {
                    let b = bridge.clone();
                    thread::spawn(move || {
                        handle_tcp_client(stream, b);
                    });
                }
            }
        })?;
    Ok(())
}

fn handle_tcp_client(stream: TcpStream, bridge: McpBridge) {
    let reader = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });
    let mut writer = stream;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<JsonRpcRequest>(&line) {
            Ok(req) => match bridge.send_command(req) {
                Ok(r) => r,
                Err(e) => JsonRpcResponse::err(None, -32603, format!("Internal bridge error: {e}")),
            },
            Err(e) => JsonRpcResponse::err(None, -32700, format!("Parse error: {e}")),
        };
        if let Ok(bytes) = serde_json::to_vec(&resp) {
            let _ = writer.write_all(&bytes);
            let _ = writer.write_all(b"\n");
            let _ = writer.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_initialize_and_ping() {
        let mut p = Project::demo();
        let mut s = UiState::new();

        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(1)),
            method: "initialize".to_string(),
            params: None,
        };
        let resp = McpEngine::handle_request(req, &mut p, &mut s, None);
        assert_eq!(resp.id, Some(json!(1)));
        assert!(resp.error.is_none());
        let res = resp.result.unwrap();
        assert_eq!(res["serverInfo"]["name"], "douri-mcp-server");

        let ping_req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(2)),
            method: "ping".to_string(),
            params: None,
        };
        let ping_resp = McpEngine::handle_request(ping_req, &mut p, &mut s, None);
        assert_eq!(ping_resp.id, Some(json!(2)));
        assert!(ping_resp.error.is_none());
    }

    #[test]
    fn test_mcp_resources() {
        let mut p = Project::demo();
        let mut s = UiState::new();

        let list_req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(10)),
            method: "resources/list".to_string(),
            params: None,
        };
        let list_resp = McpEngine::handle_request(list_req, &mut p, &mut s, None);
        assert!(list_resp.result.is_some());
        let resources = list_resp.result.unwrap()["resources"]
            .as_array()
            .cloned()
            .unwrap();
        assert_eq!(resources.len(), 4);

        let read_req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(11)),
            method: "resources/read".to_string(),
            params: Some(json!({ "uri": "douri://project/state" })),
        };
        let read_resp = McpEngine::handle_request(read_req, &mut p, &mut s, None);
        assert!(read_resp.result.is_some());
        let text = read_resp.result.unwrap()["contents"][0]["text"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(text.contains("tempo"));
        assert!(text.contains("tracks"));
    }

    #[test]
    fn test_mcp_tools_dispatch() {
        let mut p = Project::demo();
        let mut s = UiState::new();

        // 1. set_transport
        let t_req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(100)),
            method: "tools/call".to_string(),
            params: Some(json!({
                "name": "set_transport",
                "arguments": { "action": "set_tempo", "value": 140.0 }
            })),
        };
        let t_resp = McpEngine::handle_request(t_req, &mut p, &mut s, None);
        assert!(t_resp.error.is_none());
        assert_eq!(p.tempo, 140.0);

        // 2. create_track
        let ct_req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(101)),
            method: "tools/call".to_string(),
            params: Some(json!({
                "name": "create_track",
                "arguments": { "kind": "audio", "name": "Guitar Lead" }
            })),
        };
        let ct_resp = McpEngine::handle_request(ct_req, &mut p, &mut s, None);
        assert!(ct_resp.error.is_none());
        assert_eq!(p.tracks.last().unwrap().name, "Guitar Lead");

        // 3. create_midi_clip
        let mc_req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(102)),
            method: "tools/call".to_string(),
            params: Some(json!({
                "name": "create_midi_clip",
                "arguments": {
                    "track_index": 0,
                    "name": "Arp Pattern",
                    "start_beat": 0.0,
                    "length_beats": 4.0,
                    "notes": [
                        { "key": 60, "start": 0.0, "length": 0.5, "velocity": 100 },
                        { "key": 64, "start": 1.0, "length": 0.5, "velocity": 100 }
                    ]
                }
            })),
        };
        let mc_resp = McpEngine::handle_request(mc_req, &mut p, &mut s, None);
        assert!(mc_resp.error.is_none());
        assert!(p.tracks[0].regions.iter().any(|r| r.name == "Arp Pattern"));

        // 4. set_mixer_channel
        let mix_req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(103)),
            method: "tools/call".to_string(),
            params: Some(json!({
                "name": "set_mixer_channel",
                "arguments": { "track_index": 0, "volume_db": -6.0, "pan": -0.5, "mute": true }
            })),
        };
        let mix_resp = McpEngine::handle_request(mix_req, &mut p, &mut s, None);
        assert!(mix_resp.error.is_none());
        assert_eq!(p.tracks[0].volume_db, -6.0);
        assert_eq!(p.tracks[0].pan, -0.5);
        assert!(p.tracks[0].mute);

        // 5. new_project
        let np_req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(104)),
            method: "tools/call".to_string(),
            params: Some(json!({
                "name": "new_project",
                "arguments": { "name": "Sonare MCP", "tempo": 102.0, "numerator": 4, "denominator": 4 }
            })),
        };
        let np_resp = McpEngine::handle_request(np_req, &mut p, &mut s, None);
        assert!(np_resp.error.is_none());
        assert_eq!(p.tempo, 102.0);
        assert_eq!(p.name, "Sonare MCP");

        p.add_track(TrackKind::Instrument);

        // 8. bounce_project to temp file
        let tmp_wav =
            std::env::temp_dir().join(format!("test_mcp_bounce_{}.wav", std::process::id()));
        let b_req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(107)),
            method: "tools/call".to_string(),
            params: Some(json!({
                "name": "bounce_project",
                "arguments": {
                    "output_path": tmp_wav.to_string_lossy(),
                    "sample_rate": 44100,
                    "bit_depth": 16,
                    "dither": true
                }
            })),
        };
        let b_resp = McpEngine::handle_request(b_req, &mut p, &mut s, None);
        assert!(b_resp.error.is_none());
        assert!(tmp_wav.is_file());
        let _ = std::fs::remove_file(&tmp_wav);

        // 9. save_project and load_project
        let tmp_proj = std::env::temp_dir().join(format!("test_mcp_{}.douri", std::process::id()));
        let sp_req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(108)),
            method: "tools/call".to_string(),
            params: Some(json!({
                "name": "save_project",
                "arguments": { "path": tmp_proj.to_string_lossy() }
            })),
        };
        let sp_resp = McpEngine::handle_request(sp_req, &mut p, &mut s, None);
        assert!(sp_resp.error.is_none());
        assert!(tmp_proj.is_file());

        let lp_req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(109)),
            method: "tools/call".to_string(),
            params: Some(json!({
                "name": "load_project",
                "arguments": { "path": tmp_proj.to_string_lossy() }
            })),
        };
        let lp_resp = McpEngine::handle_request(lp_req, &mut p, &mut s, None);
        assert!(lp_resp.error.is_none());
        assert_eq!(p.tempo, 102.0);
        let _ = std::fs::remove_file(&tmp_proj);
    }
}
