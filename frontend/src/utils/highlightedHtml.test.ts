// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import {
  dedentHighlightedHtmlBlock,
  dedentHighlightedHtmlLines,
  splitHighlightedHtml,
  stripHtmlTags,
} from "./highlightedHtml";

describe("dedentHighlightedHtmlLines", () => {
  it("removes shared indentation while preserving continuation indentation", () => {
    const lines = [
      "    <a-k>let</a-k> (mut server_session, server_handle) = acceptor(session)",
      "        .<a-f>establish</a-f>()",
      "        .<a-f>await</a-f>",
      '        .<a-f>expect</a-f>("server handshake failed");',
    ];

    const dedented = dedentHighlightedHtmlLines(lines).map((line) => stripHtmlTags(line));

    expect(dedented).toEqual([
      "let (mut server_session, server_handle) = acceptor(session)",
      "    .establish()",
      "    .await",
      '    .expect("server handshake failed");',
    ]);
  });
});

describe("dedentHighlightedHtmlBlock", () => {
  it("dedents multiline highlighted blocks without stripping tags", () => {
    const html =
      "    <a-k>let</a-k> value = session\n        .<a-f>establish</a-f>()\n        .<a-f>await</a-f>;";

    const dedented = dedentHighlightedHtmlBlock(html);
    const lines = splitHighlightedHtml(dedented);

    expect(lines[1]).toContain("<a-f>establish</a-f>");
    expect(lines.map((line) => stripHtmlTags(line))).toEqual([
      "let value = session",
      "    .establish()",
      "    .await;",
    ]);
  });
});
