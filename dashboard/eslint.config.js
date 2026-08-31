/* Lint rules for the dashboard.
 *
 * Deliberately close to the defaults: the recommended JS set plus the React
 * hooks rules. The value here is the class of bug a reviewer does not catch by
 * reading - an unused import left behind by a refactor, a `case` that falls
 * through, a hook called inside a branch - not house style. There is no
 * formatter rule in here on purpose. */

import js from "@eslint/js";
import reactHooks from "eslint-plugin-react-hooks";
import globals from "globals";

export default [
  { ignores: ["dist/**"] },
  js.configs.recommended,
  reactHooks.configs.flat["recommended-latest"],
  {
    files: ["**/*.{js,jsx}"],
    languageOptions: {
      ecmaVersion: "latest",
      sourceType: "module",
      // Flat config has no `env`, so the browser and Node names the app really
      // uses have to be declared or `no-undef` reports every one of them.
      globals: { ...globals.browser, ...globals.node },
      parserOptions: { ecmaFeatures: { jsx: true } },
    },
    rules: {
      // `const { secret, ...rest } = row` is how this codebase strips a field
      // it must not send. Without this the omitted name reads as dead code and
      // the fix is to stop omitting it, which is the opposite of the intent.
      "no-unused-vars": ["error", { ignoreRestSiblings: true }],
      // Off: it fires on `useEffect(() => { if (live) load(); }, [live])` where
      // `load` is async, so the setState runs after an await and not in the
      // effect body the rule describes. Load-on-mount is the pattern here and
      // there are seven of them; keeping this on would mean seven suppressions.
      "react-hooks/set-state-in-effect": "off",
    },
  },
];
