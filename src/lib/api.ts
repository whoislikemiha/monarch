/**
 * Unified IPC wrapper — auto-detects Tauri webview vs plain browser.
 *
 * In Tauri: delegates to @tauri-apps/api/{core,event}.
 * In browser: uses a WebSocket connection to the Rust backend (ws://localhost:3001).
 *
 * All frontend code should import invoke/listen from here, not from @tauri-apps/api.
 */

export type UnlistenFn = () => void;

export interface Event<T = unknown> {
  payload: T;
}

const isTauri =
  typeof window !== "undefined" &&
  typeof (window as any).__TAURI_INTERNALS__ !== "undefined";

// ---- Tauri path ----

let tauriInvoke: typeof import("@tauri-apps/api/core").invoke | undefined;
let tauriListen: typeof import("@tauri-apps/api/event").listen | undefined;

if (isTauri) {
  // Dynamic imports so the modules are only loaded inside the webview
  import("@tauri-apps/api/core").then((m) => (tauriInvoke = m.invoke));
  import("@tauri-apps/api/event").then((m) => (tauriListen = m.listen));
}

// ---- WebSocket path ----

type PendingRequest = {
  resolve: (value: any) => void;
  reject: (reason: string) => void;
};

type EventCallback = (event: Event) => void;

let ws: WebSocket | null = null;
let wsReady: Promise<void> | null = null;
let nextId = 1;
const pending = new Map<number, PendingRequest>();
const listeners = new Map<string, Set<EventCallback>>();

function getWsUrl(): string {
  const port =
    (typeof window !== "undefined" &&
      (window as any).__MONARCH_WS_PORT__) ||
    3001;
  return `ws://127.0.0.1:${port}`;
}

function ensureWs(): Promise<void> {
  if (ws && ws.readyState === WebSocket.OPEN) return Promise.resolve();
  if (wsReady) return wsReady;

  wsReady = new Promise<void>((resolve, reject) => {
    const socket = new WebSocket(getWsUrl());

    socket.onopen = () => {
      ws = socket;
      // Re-subscribe all active event listeners
      for (const event of listeners.keys()) {
        socket.send(JSON.stringify({ cmd: "listen", args: { event } }));
      }
      resolve();
    };

    socket.onerror = (e) => {
      console.error("[monarch-ws] Connection error", e);
      reject(new Error("WebSocket connection failed"));
    };

    socket.onclose = () => {
      ws = null;
      wsReady = null;
      // Reject all pending requests
      for (const [id, req] of pending) {
        req.reject("WebSocket closed");
        pending.delete(id);
      }
      // Attempt reconnect after a short delay
      setTimeout(() => {
        if (listeners.size > 0) {
          ensureWs().catch(() => {});
        }
      }, 2000);
    };

    socket.onmessage = (msg) => {
      try {
        const data = JSON.parse(msg.data);
        if (data.event) {
          // Server-pushed event
          const cbs = listeners.get(data.event);
          if (cbs) {
            const event: Event = { payload: data.payload };
            for (const cb of cbs) cb(event);
          }
        } else if (data.id != null) {
          // Response to a request
          const req = pending.get(data.id);
          if (req) {
            pending.delete(data.id);
            if (data.error) {
              req.reject(data.error);
            } else {
              req.resolve(data.result);
            }
          }
        }
      } catch {
        // Ignore parse errors
      }
    };
  });

  wsReady.catch(() => {
    wsReady = null;
  });

  return wsReady;
}

async function wsInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  await ensureWs();
  return new Promise<T>((resolve, reject) => {
    const id = nextId++;
    pending.set(id, { resolve, reject });
    ws!.send(JSON.stringify({ id, cmd, args: args || {} }));
  });
}

async function wsListen<T>(
  event: string,
  handler: (event: Event<T>) => void,
): Promise<UnlistenFn> {
  await ensureWs();

  if (!listeners.has(event)) {
    listeners.set(event, new Set());
    // Tell the server we want this event
    ws!.send(JSON.stringify({ cmd: "listen", args: { event } }));
  }

  const cbs = listeners.get(event)!;
  const cb = handler as EventCallback;
  cbs.add(cb);

  return () => {
    cbs.delete(cb);
    if (cbs.size === 0) {
      listeners.delete(event);
      if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ cmd: "unlisten", args: { event } }));
      }
    }
  };
}

// ---- Public API ----

export async function invoke<T = unknown>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (isTauri) {
    // Wait for dynamic import to settle on first call
    if (!tauriInvoke) {
      const mod = await import("@tauri-apps/api/core");
      tauriInvoke = mod.invoke;
    }
    return tauriInvoke<T>(cmd, args);
  }
  return wsInvoke<T>(cmd, args);
}

export async function listen<T = unknown>(
  event: string,
  handler: (event: Event<T>) => void,
): Promise<UnlistenFn> {
  if (isTauri) {
    if (!tauriListen) {
      const mod = await import("@tauri-apps/api/event");
      tauriListen = mod.listen;
    }
    return tauriListen<T>(event, handler) as Promise<UnlistenFn>;
  }
  return wsListen<T>(event, handler);
}
