# Rolling live history

Session capture uses bounded suffix retention. Strict `Recording::append` is for validated file imports and fixtures; using it for the live session permanently stops capture on normal quota exhaustion. This focused rule catches that regression, while session tests verify eviction, presentation alignment and a frozen historical reader.
