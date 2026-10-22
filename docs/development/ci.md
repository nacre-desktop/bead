# CI runners

Bead's `.github/workflows/ci.yml` `check` job runs for pushes to
`refs/heads/main` and maintainer-started `workflow_dispatch` runs. Its job guard
skips dispatches on any other ref. It runs in a resource-capped Ubuntu 24.04
container capped at 10 CPUs and 28 GiB on the self-hosted runner, and selects
the `nacre-ci` organization runner group through the `NACRE_CI_RUNNER`
repository variable. The cap is omitted for the hosted `ubuntu-24.04` fallback,
which uses its hosted VM's resource limits. The other group consumer is
Nacre's `.github/workflows/check.yml` `desktop` and `fresh-install` jobs, which
run on pushes and maintainer-started `workflow_dispatch` at `refs/heads/main`,
with job guards skipping dispatches on other refs. Scope the group to exactly
`nacre-desktop/nacre/.github/workflows/check.yml@refs/heads/main` and
`nacre-desktop/bead/.github/workflows/ci.yml@refs/heads/main`.
`compositor-hardware.yml` uses a separate runner label and is outside this group.

Restrict the organization runner group to the selected repositories
`nacre-desktop/nacre` and `nacre-desktop/bead`, and enable its access to public
repositories. Require approval for all outside collaborators' fork pull
requests in both repositories. Workflows assigned to this group must not run
for `pull_request`, `pull_request_target`, or `issue_comment`. Recheck the exact
workflow paths and triggers before adding another group consumer.

The prod runners are ephemeral containers. Each runner registers for one job,
then exits; the container supervisor starts a fresh runner. They have access to
the host Docker socket for container jobs, so workflows assigned to this group
must remain limited to approved main-branch pushes and trusted maintainers.
GitHub Actions caches remain repository-scoped. Do not mount a shared host
cargo or package cache into job containers; add only repo-isolated caches after
reviewing their trust boundaries.

To temporarily use hosted runners, set the repository variable
`NACRE_CI_RUNNER` to the JSON array `["ubuntu-24.04"]`. Remove the variable to
return to the self-hosted default
`["self-hosted","linux","nacre-ci"]`.
