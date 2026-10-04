# Frozen selection uses visible edges

Frozen grids are intentionally immutable across resizing; live PTY dimensions
continue to change. After shrinking, painting clips the old grid. Pointer
endpoints and edge autoscroll must therefore use the intersection of grid and
canvas, not the old snapshot's bottom hidden beneath the composer.

This detector rejects the previous full-grid height passed directly into
`edge_scroll_lines`. Geometry unit tests and graphical frozen/shrink/held-drag
checks provide the behavioral oracle; this rule is not a general geometry proof.
