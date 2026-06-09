# Frontend Design Eval Prompts

Use these as lightweight trigger checks when changing this skill.

```json
[
  {
    "prompt": "The sessions right panel feels noisy and hard to scan. Improve the visual hierarchy without changing backend behavior.",
    "should_trigger": true,
    "expected_behavior": "Use frontend-design in operational app mode and validate with agent-browser; preserve density."
  },
  {
    "prompt": "Make the task detail view responsive on mobile; tabs and action buttons currently overflow.",
    "should_trigger": true,
    "expected_behavior": "Inspect nearby Svelte patterns, add stable responsive constraints, validate with agent-browser."
  },
  {
    "prompt": "Add a new Rust field and regenerate TypeScript bindings.",
    "should_trigger": false,
    "expected_behavior": "Use backend/Rust and ts-rs workflow; frontend-design only applies if UI layout changes are requested."
  }
]
```
