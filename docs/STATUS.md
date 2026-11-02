# Framework status

Bead is an independent MIT framework with a standalone minimal-shell example. B0214 steps 3 and 4 record the product history import into Nacre and its removal from Bead; closure checks for step 5 are tracked in the split decision.

Framework CI and the minimal-shell proof define the current build contract. See [the CI guide](development/ci.md), [spec index](specs/README.md), and [framework direction](ROADMAP.md).

Full framework CI passed on `b18cec0ce64378a9db8218f227eeeb75b289fb13` without an application checkout: formatting, Clippy, workspace tests, renderer features, neutral examples, private service buses and all 12 independent dependency/license roots. Product import/path/command negative controls also passed.

Read-only Nacre compatibility also passed at `8c56db8960b63b6d4cfabd98e2c281c85c03e68c`: workspace build and shell all-features check against its unchanged Bead pin `f19eacaf7b3dedfbb54202358d88e699444366b1`. No Nacre source or installation changed.
