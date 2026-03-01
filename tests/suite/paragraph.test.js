// @ts-check

"use strict";

const { assert, describe, test } = require("../runner"),
  { ParagraphBuilder } = require("../../lib");

describe("ParagraphBuilder", () => {
  test("can create with empty style (no crash on missing optional fields)", () => {
    let pb = ParagraphBuilder.Make({});
    assert.ok(pb, "ParagraphBuilder should be created with empty style");
  });

  test("can create with partial style", () => {
    let pb = ParagraphBuilder.Make({ textAlign: "center" });
    assert.ok(pb, "ParagraphBuilder should accept partial style");
  });

  test("can create with full style and build a paragraph", () => {
    let pb = ParagraphBuilder.Make({
      textAlign: "left",
      textDirection: "ltr",
      maxLines: 3,
      ellipsis: "...",
      textStyle: {
        fontSize: 16,
        fontFamilies: ["sans-serif"],
        color: "black",
      },
    });
    pb.addText("Hello world");
    let paragraph = pb.build();
    assert.ok(paragraph, "Paragraph should be built");
    paragraph.layout(200);
    assert.ok(
      paragraph.getHeight() > 0,
      "Paragraph should have height after layout",
    );
  });

  test("can push and pop styles", () => {
    let pb = ParagraphBuilder.Make({});
    pb.pushStyle({ fontSize: 24, color: "red" });
    pb.addText("Bold ");
    pb.pop();
    pb.addText("Normal");
    let paragraph = pb.build();
    paragraph.layout(400);
    assert.ok(paragraph.getHeight() > 0);
  });
});
