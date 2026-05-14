/**
 * Extension UI bridge — implements ExtensionUIContext by forwarding
 * interactive requests to Rust via stdout and resolving when responses arrive.
 *
 * Same pattern as Pi's own RPC server (rpc-mode.ts createExtensionUIContext).
 */

import type { ExtensionUIContext } from "@mariozechner/pi-coding-agent";
import { randomUUID } from "node:crypto";

export type EmitFn = (msg: Record<string, unknown>) => void;
export type UIResolvers = Map<string, (value: Record<string, unknown>) => void>;

export function createUIBridge(
	agentId: string,
	emit: EmitFn,
	resolvers: UIResolvers,
): ExtensionUIContext {
	function emitRequest(
		method: string,
		params: Record<string, unknown>,
		requestId?: string,
	): string {
		const id = requestId ?? randomUUID();
		emit({
			type: "extension_ui_request",
			agentId,
			requestId: id,
			method,
			...params,
		});
		return id;
	}

	function createDialogPromise<T>(
		method: string,
		params: Record<string, unknown>,
		transform: (response: Record<string, unknown>) => T,
	): Promise<T> {
		return new Promise<T>((resolve) => {
			const id = randomUUID();
			resolvers.set(id, (response) => {
				resolvers.delete(id);
				resolve(transform(response));
			});
			emitRequest(method, params, id);
		});
	}

	return {
		select(title: string, options: string[]) {
			return createDialogPromise<string | undefined>(
				"select",
				{ title, options },
				(r) =>
					r.cancelled
						? undefined
						: typeof r.value === "string"
							? r.value
							: undefined,
			);
		},

		confirm(title: string, message: string) {
			return createDialogPromise<boolean>(
				"confirm",
				{ title, message },
				(r) => (r.cancelled ? false : r.confirmed === true),
			);
		},

		input(title: string, placeholder?: string) {
			return createDialogPromise<string | undefined>(
				"input",
				{ title, placeholder },
				(r) =>
					r.cancelled
						? undefined
						: typeof r.value === "string"
							? r.value
							: undefined,
			);
		},

		async editor(title: string, prefill?: string) {
			return createDialogPromise<string | undefined>(
				"editor",
				{ title, prefill },
				(r) =>
					r.cancelled
						? undefined
						: typeof r.value === "string"
							? r.value
							: undefined,
			);
		},

		notify(message: string, type?: "info" | "warning" | "error") {
			emitRequest("notify", { message, notifyType: type });
		},

		setStatus(key: string, text: string | undefined) {
			emitRequest("setStatus", { statusKey: key, statusText: text });
		},

		setTitle(title: string) {
			emitRequest("setTitle", { title });
		},

		setEditorText(text: string) {
			emitRequest("set_editor_text", { text });
		},

		// No-ops for TUI-only features
		onTerminalInput() {
			return () => {};
		},
		setWorkingMessage() {},
		setWorkingIndicator() {},
		setWorkingVisible() {},
		addAutocompleteProvider() {},
		setHiddenThinkingLabel() {},
		setWidget() {},
		setFooter() {},
		setHeader() {},
		setEditorComponent() {},
		getEditorComponent() {
			return undefined;
		},
		pasteToEditor() {},
		getEditorText() {
			return "";
		},
		setTheme() {
			return { success: false, error: "Not supported in sidecar" };
		},
		getTheme() {
			return undefined;
		},
		getAllThemes() {
			return [];
		},
		get theme(): any {
			return {};
		},
		getToolsExpanded() {
			return false;
		},
		setToolsExpanded() {},
		async custom() {
			return undefined as never;
		},
	} satisfies ExtensionUIContext;
}
