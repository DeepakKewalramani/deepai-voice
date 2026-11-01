/**
 * "Make your own" field rules. Mirrors src-tauri/src/providers/custom.rs, which
 * stays the authority; both run validation-vectors.json. Messages never echo input.
 */

export type Check =
  { ok: true; value: string } | { ok: false; message: string };

const ok = (value: string): Check => ({ ok: true, value });
const bad = (message: string): Check => ({ ok: false, message });

const hasControl = (s: string) => [...s].some((c) => /\p{Cc}/u.test(c));

export function validateName(name: string): Check {
  const t = name.trim();
  if (t.length === 0) return bad("Enter a name.");
  if ([...t].length > 40)
    return bad("Use 40 characters or fewer for the name.");
  if (hasControl(t)) return bad("The name cannot contain line breaks or tabs.");
  return ok(t);
}

export function validateModel(model: string): Check {
  const t = model.trim();
  if (t.length === 0) return bad("Enter the model id, for example llama3.2.");
  if ([...t].length > 80)
    return bad("Use 80 characters or fewer for the model id.");
  if (/\s/u.test(t) || hasControl(t))
    return bad("The model id cannot contain spaces.");
  return ok(t);
}

const RESERVED = [
  "host",
  "content-length",
  "content-type",
  "transfer-encoding",
  "connection",
  "cookie",
  "user-agent",
];

export function validateHeaderName(name: string): Check {
  const t = name.trim();
  if (t.length === 0)
    return bad("Enter the header name, for example api-subscription-key.");
  if (t.length > 64)
    return bad("Use 64 characters or fewer for the header name.");
  if (!/^[A-Za-z0-9!#$%&'*+\-.^_`|~]+$/.test(t))
    return bad("Use only letters, digits and - _ . in the header name.");
  if (RESERVED.includes(t.toLowerCase()))
    return bad("That header is managed by the app. Choose another name.");
  return ok(t);
}

const LOCAL_HOSTS = ["localhost", "127.0.0.1", "[::1]"];
const GENERIC =
  "Enter a full address that starts with https://, for example https://api.openai.com/v1.";
const BAD_HOST = "The server name in the address is not valid.";

function splitHostPort(
  authority: string,
): { host: string; port: string | null } | string {
  let host: string;
  let port: string | null = null;
  if (authority.startsWith("[")) {
    const end = authority.indexOf("]");
    if (end < 0) return BAD_HOST;
    const inner = authority.slice(1, end);
    if (inner.length === 0 || !/^[0-9A-Fa-f:.]+$/.test(inner)) return BAD_HOST;
    host = authority.slice(0, end + 1);
    const after = authority.slice(end + 1);
    if (after !== "") {
      if (!after.startsWith(":")) return BAD_HOST;
      port = after.slice(1);
    }
  } else {
    const i = authority.indexOf(":");
    host = i < 0 ? authority : authority.slice(0, i);
    port = i < 0 ? null : authority.slice(i + 1);
  }
  if (host.length === 0) return BAD_HOST;
  if (!host.startsWith("[")) {
    const labelOk = (l: string) =>
      l.length > 0 &&
      l.length <= 63 &&
      /^[A-Za-z0-9-]+$/.test(l) &&
      !l.startsWith("-") &&
      !l.endsWith("-");
    if (host.length > 253 || !host.split(".").every(labelOk)) return BAD_HOST;
  }
  if (port !== null) {
    const n = /^[0-9]+$/.test(port) ? Number(port) : NaN;
    if (!(n >= 1 && n <= 65535))
      return "The port must be a number from 1 to 65535.";
    port = String(n);
  }
  return { host, port };
}

/** Same rules and normalisation as `validate_base_url` in Rust. */
export function validateBaseUrl(raw: string): Check {
  const s = raw.trim();
  if (s.length === 0) return bad(GENERIC);
  if (s.length > 2048) return bad("The address is too long.");
  if (/[\s\\]/u.test(s) || hasControl(s))
    return bad("The address cannot contain spaces or backslashes.");
  if (s.includes("?") || s.includes("#"))
    return bad("Remove the ? or # part. Keys never go in the address.");
  const sep = s.indexOf("://");
  if (sep < 0) return bad(GENERIC);
  const scheme = s.slice(0, sep).toLowerCase();
  if (scheme !== "https" && scheme !== "http") return bad(GENERIC);
  const rest = s.slice(sep + 3);
  const slash = rest.indexOf("/");
  const authority = slash < 0 ? rest : rest.slice(0, slash);
  const path = slash < 0 ? "" : rest.slice(slash);
  if (authority.includes("@"))
    return bad(
      "Remove the user name or password from the address. Keys go in the key field.",
    );
  const hp = splitHostPort(authority);
  if (typeof hp === "string") return bad(hp);
  const host = hp.host.toLowerCase();
  if (scheme === "http" && !LOCAL_HOSTS.includes(host))
    return bad(
      "Use https://. Plain http:// is allowed only for localhost, 127.0.0.1 or ::1.",
    );
  if (!/^[A-Za-z0-9\-._~/%:+@!$&'()*,;=]*$/.test(path))
    return bad("The address path has characters that are not allowed.");
  const port = hp.port !== null ? `:${hp.port}` : "";
  return ok(`${scheme}://${host}${port}${path.replace(/\/+$/, "")}`);
}

export type AuthChoice = "bearer" | "token" | "x-api-key" | "none" | "custom";

export interface CustomForm {
  id?: string;
  capability: "stt" | "llm" | "tts";
  name: string;
  baseUrl: string;
  model: string;
  authStyle: AuthChoice;
  headerName: string;
}

export type FormField = "name" | "baseUrl" | "model" | "headerName";
export type FormErrors = Partial<Record<FormField, string>>;

export const emptyForm: CustomForm = {
  capability: "llm",
  name: "",
  baseUrl: "",
  model: "",
  authStyle: "bearer",
  headerName: "",
};

/** Field errors for the form. Empty object when it can be saved. */
export function validateForm(f: CustomForm): FormErrors {
  const errors: FormErrors = {};
  const checks: [FormField, Check][] = [
    ["name", validateName(f.name)],
    ["baseUrl", validateBaseUrl(f.baseUrl)],
    ["model", validateModel(f.model)],
  ];
  if (f.authStyle === "custom")
    checks.push(["headerName", validateHeaderName(f.headerName)]);
  for (const [field, c] of checks) if (!c.ok) errors[field] = c.message;
  return errors;
}

/** Turns a stored `authStyle` setting back into form fields. */
export function authFromSetting(s: string | null): {
  authStyle: AuthChoice;
  headerName: string;
} {
  if (s?.startsWith("header:"))
    return { authStyle: "custom", headerName: s.slice(7) };
  if (s === "token" || s === "x-api-key" || s === "none")
    return { authStyle: s, headerName: "" };
  return { authStyle: "bearer", headerName: "" };
}

export function isLocalUrl(url: string): boolean {
  const c = validateBaseUrl(url);
  if (!c.ok) return false;
  const authority = c.value.split("://")[1].split("/")[0];
  const hp = splitHostPort(authority);
  return typeof hp !== "string" && LOCAL_HOSTS.includes(hp.host.toLowerCase());
}
