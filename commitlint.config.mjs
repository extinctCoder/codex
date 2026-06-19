// Conventional Commits ruleset for commit messages.
// Docs: https://www.conventionalcommits.org
// Enforced in CI by .github/workflows/commitlint.yml
export default {
  extends: ["@commitlint/config-conventional"],
  rules: {
    // Allowed commit types. Format: <type>(optional scope): <subject>
    "type-enum": [
      2,
      "always",
      [
        "feat", // a new feature
        "fix", // a bug fix
        "docs", // documentation only
        "style", // formatting, no code change
        "refactor", // code change that neither fixes a bug nor adds a feature
        "perf", // performance improvement
        "test", // adding or fixing tests
        "build", // build system or dependencies
        "ci", // CI configuration
        "chore", // other changes that don't modify src or test files
        "revert", // reverts a previous commit
      ],
    ],
    "subject-case": [0], // allow any case in the subject line
    "header-max-length": [2, "always", 100],
  },
};
