# ADR 0002 — View authoring format

Status: accepted.

Views use Luau through the framework's existing runtime. This keeps view code separate from platform and rendering code while supporting validation before a new generation is installed. The framework owns the runtime integration and generic UI modules; each application owns its view tree, catalogs, assets, and application helpers.

The host controls the exposed APIs and plugin capability policy. No product routes or service contracts belong in the framework authoring format.
