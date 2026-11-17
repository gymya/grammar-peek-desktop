import test from "node:test";
import assert from "node:assert/strict";
import { correctText } from "../src/corrections.js";
const edit = (offset, length, value) => ({ offset, length, replacements: [{ value }] });

test("combines original offsets without shifting later corrections", () => {
  assert.equal(correctText("This are an test.", [edit(9, 2, "a"), edit(5, 3, "is")]).text, "This is a test.");
});
test("handles UTF-16 offsets, insertion and deletion", () => {
  assert.equal(correctText("😀 a test!", [edit(3, 1, "the"), edit(9, 1, "")]).text, "😀 the test");
  assert.equal(correctText("Hello world", [edit(5, 0, ",")]).text, "Hello, world");
});
test("skips overlapping, invalid and unavailable suggestions", () => {
  const output = correctText("bad text", [edit(0, 3, "good"), edit(1, 2, "x"), edit(99, 1, "x"), { offset: 4, length: 4, replacements: [] }]);
  assert.deepEqual(output, { text: "good text", applied: 1, unresolved: 3,
    changes: [{ start: 0, length: 4, original: "bad" }] });
});
test("preserves text when there are no matches", () => {
  assert.deepEqual(correctText("A good sentence.", []), { text: "A good sentence.", applied: 0, unresolved: 0, changes: [] });
});
test("highlight offsets refer to corrected text after earlier length changes", () => {
  const output = correctText("This are an test.", [edit(9, 2, "a"), edit(5, 3, "is")]);
  assert.deepEqual(output.changes, [
    { start: 5, length: 2, original: "are" },
    { start: 8, length: 1, original: "an" }
  ]);
  assert.deepEqual(output.changes.map(c => output.text.slice(c.start, c.start + c.length)), ["is", "a"]);
});
test("tracks inserted punctuation and removed text without adding it to the corrected sentence", () => {
  const output = correctText("Hello world!", [edit(5, 0, ","), edit(11, 1, "")]);
  assert.equal(output.text, "Hello, world");
  assert.deepEqual(output.changes, [
    { start: 5, length: 1, original: "" },
    { start: 12, length: 0, original: "!" }
  ]);
});
