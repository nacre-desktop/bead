# Framework components

Luau views can import `bead.kit`; reusable components are embedded from the MIT `bead-kit` crate through `bead-ui`. Hosts may use these components or build custom primitives with `bead.ui`.

Hosts provide theme tokens, gradients, and font-family choices through the theme API. Invalid theme data leaves the last valid theme active. The framework provides neutral defaults and does not synchronize its values with an application repository.

`bead-widgets` provides reusable forms, placement, drag geometry, measurement, virtual-list modules, and an opaque atomic JSON/revision store. Hosts supply catalogs, bindings, actions, geometry policies, animation settings, and storage paths.

The kit includes buttons, icons, sliders, switches, tiles, segmented controls, lists, cards, popovers, tooltips, and text inputs. Components retain stable identity across reloads where their host bindings use stable IDs. Framework widgets do not add plugin capabilities or application routes.
