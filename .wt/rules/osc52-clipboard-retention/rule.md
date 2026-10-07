# OSC 52 clipboard retention

The live terminal may receive untrusted OSC 52 content. Kea retains only the
latest accepted store, applies a byte limit before queue retention, and reports
oversized requests. The upstream VTE parser can still transiently process a
larger OSC string; this rule protects the Kea-owned retained queue and does not
claim to bound parser memory.
