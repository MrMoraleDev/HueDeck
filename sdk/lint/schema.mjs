// SPDX-License-Identifier: GPL-3.0-or-later
//
// The subset of JSON Schema (draft 2020-12) that HueDeck's own schemas use, with no
// dependencies: type, enum, const, pattern, minLength/maxLength, minimum/maximum,
// minItems/maxItems, uniqueItems, items, properties, required, additionalProperties,
// anyOf/oneOf/allOf/not, if/then/else, boolean schemas and local `$ref`s ("#/$defs/x").
// `format`, `default`, `description` and `examples` are annotations and are not checked.
//
// validate(schema, value) returns [] or a list of { path, message }, where `path` is a JSON
// pointer into `value` ("" for the root, "/devices/0/vid").

/** @typedef {{ path: string, message: string }} SchemaError */

const isObject = (v) => v !== null && typeof v === "object" && !Array.isArray(v);

function typeOf(v) {
  if (v === null) return "null";
  if (Array.isArray(v)) return "array";
  if (typeof v === "number") return Number.isInteger(v) ? "integer" : "number";
  return typeof v;
}

function hasType(v, type) {
  const t = typeOf(v);
  return t === type || (type === "number" && t === "integer");
}

function deepEqual(a, b) {
  if (a === b) return true;
  if (typeof a !== typeof b || a === null || b === null || typeof a !== "object") return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  if (Array.isArray(a)) return a.length === b.length && a.every((x, i) => deepEqual(x, b[i]));
  const ka = Object.keys(a);
  const kb = Object.keys(b);
  return ka.length === kb.length && ka.every((k) => Object.hasOwn(b, k) && deepEqual(a[k], b[k]));
}

const show = (v) => {
  const text = JSON.stringify(v);
  return text === undefined ? String(v) : text.length > 60 ? `${text.slice(0, 57)}...` : text;
};

const pointer = (path, key) => `${path}/${String(key).replace(/~/g, "~0").replace(/\//g, "~1")}`;

/** Resolve a local `$ref` ("#/$defs/usbId") against the root schema. */
function resolveRef(root, ref) {
  if (!ref.startsWith("#")) throw new Error(`only local $ref is supported, got ${ref}`);
  let node = root;
  for (const raw of ref.slice(1).split("/").filter(Boolean)) {
    const key = decodeURIComponent(raw).replace(/~1/g, "/").replace(/~0/g, "~");
    node = node?.[key];
    if (node === undefined) throw new Error(`unresolvable $ref ${ref}`);
  }
  return node;
}

const patterns = new Map();
function regex(pattern) {
  let re = patterns.get(pattern);
  if (re === undefined) {
    re = new RegExp(pattern);
    patterns.set(pattern, re);
  }
  return re;
}

/**
 * Validate `value` against `schema` (whose `$defs` serve `$ref`s; pass `root` when `schema`
 * is a sub-schema of another document).
 * @returns {SchemaError[]}
 */
export function validate(schema, value, root = schema) {
  const errors = [];
  check(schema, value, "", root, errors);
  return errors;
}

function check(schema, value, path, root, errors) {
  if (schema === true || schema === undefined) return;
  if (schema === false) {
    errors.push({ path, message: "is not allowed here" });
    return;
  }
  if (schema.$ref !== undefined) {
    check(resolveRef(root, schema.$ref), value, path, root, errors);
  }
  if (schema.type !== undefined) {
    const types = Array.isArray(schema.type) ? schema.type : [schema.type];
    if (!types.some((t) => hasType(value, t))) {
      errors.push({ path, message: `must be ${types.join(" or ")} (got ${typeOf(value)} ${show(value)})` });
      return;
    }
  }
  if (schema.const !== undefined && !deepEqual(schema.const, value)) {
    errors.push({ path, message: `must be ${show(schema.const)} (got ${show(value)})` });
  }
  if (schema.enum !== undefined && !schema.enum.some((e) => deepEqual(e, value))) {
    errors.push({ path, message: `must be one of ${schema.enum.map(show).join(", ")} (got ${show(value)})` });
  }
  if (typeof value === "string") {
    const length = [...value].length;
    if (schema.minLength !== undefined && length < schema.minLength) {
      errors.push({ path, message: `must have at least ${schema.minLength} characters` });
    }
    if (schema.maxLength !== undefined && length > schema.maxLength) {
      errors.push({ path, message: `must have at most ${schema.maxLength} characters` });
    }
    if (schema.pattern !== undefined && !regex(schema.pattern).test(value)) {
      errors.push({ path, message: `${show(value)} does not match ${schema.pattern}` });
    }
  }
  if (typeof value === "number") {
    if (schema.minimum !== undefined && value < schema.minimum) {
      errors.push({ path, message: `must be >= ${schema.minimum} (got ${value})` });
    }
    if (schema.maximum !== undefined && value > schema.maximum) {
      errors.push({ path, message: `must be <= ${schema.maximum} (got ${value})` });
    }
  }
  if (Array.isArray(value)) {
    if (schema.minItems !== undefined && value.length < schema.minItems) {
      errors.push({ path, message: `must have at least ${schema.minItems} item(s)` });
    }
    if (schema.maxItems !== undefined && value.length > schema.maxItems) {
      errors.push({ path, message: `must have at most ${schema.maxItems} item(s)` });
    }
    if (schema.uniqueItems === true) {
      for (let i = 0; i < value.length; i++) {
        for (let j = i + 1; j < value.length; j++) {
          if (deepEqual(value[i], value[j])) {
            errors.push({ path: pointer(path, j), message: `duplicates item ${i} (${show(value[i])})` });
          }
        }
      }
    }
    if (schema.items !== undefined) {
      value.forEach((item, i) => check(schema.items, item, pointer(path, i), root, errors));
    }
  }
  if (isObject(value)) {
    for (const key of schema.required ?? []) {
      if (!Object.hasOwn(value, key)) errors.push({ path, message: `is missing required key '${key}'` });
    }
    const props = schema.properties ?? {};
    for (const [key, v] of Object.entries(value)) {
      if (Object.hasOwn(props, key)) {
        check(props[key], v, pointer(path, key), root, errors);
      } else if (schema.additionalProperties === false) {
        errors.push({ path: pointer(path, key), message: `key '${key}' is not allowed (not in the schema)` });
      } else if (isObject(schema.additionalProperties)) {
        check(schema.additionalProperties, v, pointer(path, key), root, errors);
      }
    }
  }
  for (const sub of schema.allOf ?? []) {
    check(sub, value, path, root, errors);
  }
  if (schema.anyOf !== undefined) {
    const results = schema.anyOf.map((sub) => validate(sub, value, root).map((e) => rebase(e, path)));
    if (!results.some((r) => r.length === 0)) {
      errors.push(...closest(results, path, "any"));
    }
  }
  if (schema.oneOf !== undefined) {
    const results = schema.oneOf.map((sub) => validate(sub, value, root).map((e) => rebase(e, path)));
    const passing = results.filter((r) => r.length === 0).length;
    if (passing === 0) {
      errors.push(...closest(results, path, "one"));
    } else if (passing > 1) {
      errors.push({ path, message: `matches ${passing} of the alternatives; it must match exactly one` });
    }
  }
  if (schema.not !== undefined && validate(schema.not, value, root).length === 0) {
    errors.push({ path, message: "matches a schema it must not match" });
  }
  if (schema.if !== undefined) {
    const branch = validate(schema.if, value, root).length === 0 ? schema.then : schema.else;
    if (branch !== undefined) check(branch, value, path, root, errors);
  }
}

// Sub-validations run from the root of `value`; their paths are relative to `path`.
function rebase(error, path) {
  return { path: path + error.path, message: error.message };
}

// When no alternative matches, the errors of the closest one (fewest errors) explain it best.
function closest(results, path, word) {
  const best = results.reduce((a, b) => (b.length < a.length ? b : a));
  return [{ path, message: `does not match ${word} of the ${results.length} allowed shapes; closest:` }, ...best];
}

export { deepEqual };
