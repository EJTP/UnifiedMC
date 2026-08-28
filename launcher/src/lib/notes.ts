/**
 * Turning a release body into something readable.
 *
 * The text comes from GitHub, so it is somebody else's markdown. It is parsed into a small
 * structure the dialog renders with ordinary elements - never handed to `{@html}`. Nothing in
 * here can produce markup: the output is text and flags, and a heading is a heading because
 * the dialog draws it larger, not because a tag came out of the input.
 *
 * A deliberately small subset. Anything not understood stays as the text it was, which is the
 * behaviour a changelog wants - an unrecognised line is still worth reading.
 */

export interface Piece {
	text: string;
	bold?: boolean;
	code?: boolean;
}

export type Block =
	| { kind: "heading"; parts: Piece[] }
	| { kind: "bullet"; parts: Piece[] }
	| { kind: "row"; cells: Piece[][] }
	| { kind: "text"; parts: Piece[] };

/** `**bold**` and `` `code` ``, left as plain text wherever they are not closed. */
function inline(text: string): Piece[] {
	const parts: Piece[] = [];
	// One pass, so `**a `b` c**` keeps whichever marker opened first rather than nesting.
	const pattern = /\*\*([^*]+)\*\*|`([^`]+)`/g;
	let at = 0;
	let match: RegExpExecArray | null;

	while ((match = pattern.exec(text)) !== null) {
		if (match.index > at) parts.push({ text: text.slice(at, match.index) });
		if (match[1] !== undefined) parts.push({ text: match[1], bold: true });
		else parts.push({ text: match[2], code: true });
		at = match.index + match[0].length;
	}
	if (at < text.length) parts.push({ text: text.slice(at) });
	return parts.length > 0 ? parts : [{ text }];
}

/** A table's `|---|---|` rule carries no words, only shape - and the shape is not drawn. */
function isRule(line: string): boolean {
	return /^\|?[\s|:-]+\|[\s|:-]*$/.test(line) && line.includes("-");
}

export function parseNotes(body: string, limit = 40): Block[] {
	const blocks: Block[] = [];

	for (const raw of body.split("\n")) {
		if (blocks.length >= limit) break;
		const line = raw.trim();
		if (!line || isRule(line)) continue;

		const heading = /^#{1,6}\s+(.*)$/.exec(line);
		if (heading) {
			blocks.push({ kind: "heading", parts: inline(heading[1]) });
			continue;
		}

		const bullet = /^[-*]\s+(.*)$/.exec(line);
		if (bullet) {
			blocks.push({ kind: "bullet", parts: inline(bullet[1]) });
			continue;
		}

		if (line.startsWith("|") && line.endsWith("|")) {
			const cells = line
				.slice(1, -1)
				.split("|")
				.map((cell) => inline(cell.trim()));
			// A row of nothing but empty cells is a table's invisible header; skip it.
			if (cells.some((cell) => cell.some((piece) => piece.text.trim()))) {
				blocks.push({ kind: "row", cells });
			}
			continue;
		}

		blocks.push({ kind: "text", parts: inline(line) });
	}

	return blocks;
}
