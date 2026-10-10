import {invoke} from '@tauri-apps/api/core';
// All windows must share the startup proxy; changing it for only one WebView breaks WebView2's shared profile.
export async function startupWindowProxy(){const state=await invoke('get_sdk_browser_proxy_state');return state.proxyUrl?{proxyUrl:state.proxyUrl}:{};}
