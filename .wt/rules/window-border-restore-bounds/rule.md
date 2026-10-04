# Window-border restore bounds

`WindowBounds::Maximized` and `WindowBounds::Fullscreen` carry the saved
windowed restore geometry, not the current surface dimensions. Window-border
hitboxes and resize edges must use the current viewport in surface logical
coordinates. Tiled edges are unavailable for resize requests.
