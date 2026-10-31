import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import App from "./App";

describe("App", () => {
  it("renders the product name as the heading", () => {
    expect(renderToStaticMarkup(<App />)).toContain("<h1>DeepAI Voice</h1>");
  });
});
