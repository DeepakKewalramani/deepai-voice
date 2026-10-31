import { describe, expect, it } from "vitest";
import { appTitle } from "./App";

describe("appTitle", () => {
  it("returns the product name", () => {
    expect(appTitle()).toBe("DeepAI Voice");
  });
});
