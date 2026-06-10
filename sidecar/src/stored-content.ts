import type { PromptContentPart } from "./protocol.js";

export function tryParseStoredContent(content: string): unknown {
	const trimmed = content.trim();
	if (!trimmed) return content;

	const looksSerialized =
		trimmed.startsWith("[") ||
		trimmed.startsWith("{") ||
		trimmed.startsWith("\"");
	if (!looksSerialized) return content;

	try {
		return JSON.parse(content);
	} catch {
		return content;
	}
}

export function normalizeStoredUserContent(content: string): string | Array<Record<string, unknown>> {
	const parsed = tryParseStoredContent(content);
	if (typeof parsed === "string" || Array.isArray(parsed)) {
		return parsed as string | Array<Record<string, unknown>>;
	}
	return String(parsed ?? "");
}

export function normalizeStoredAssistantContent(content: string): Array<Record<string, unknown>> {
	const parsed = tryParseStoredContent(content);
	if (Array.isArray(parsed)) {
		return parsed as Array<Record<string, unknown>>;
	}
	if (typeof parsed === "string") {
		return [{ type: "text", text: parsed }];
	}
	return [{ type: "text", text: JSON.stringify(parsed) }];
}

export function extractPromptText(message: string | PromptContentPart[]): string {
	if (typeof message === "string") return message;
	return message
		.filter((p): p is { type: "text"; text: string } => p.type === "text")
		.map((p) => p.text)
		.join("\n");
}

export function oneLine(value: string, max: number): string {
	const compact = value.replace(/\s+/g, " ").trim();
	if (compact.length <= max) return compact;
	return `${compact.slice(0, Math.max(0, max - 3)).trimEnd()}...`;
}
