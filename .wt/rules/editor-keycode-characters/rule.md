# Editor platform text input

AGENTS.md: ordinary editor text/composition must go through the component/platform input handler. In the composer adapter, review char::from or char::from_u32 calls; terminal/input.rs legitimately encodes child input and is excluded. This detects only explicit char conversions, not match-arm key maps or indirect conversions.
