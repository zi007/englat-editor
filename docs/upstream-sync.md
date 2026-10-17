# Upstream sync

The weekly workflow `.github/workflows/upstream-watch.yml` (Mondays, also runnable by hand) keeps one
open issue, "Upstream changes", that lists new commits in the upstream projects this fork is built
from. It never changes code and never deploys.

Merging is a reviewed task because the fork is rebranded (names, links, removed upstream-community
features) and uses vendored snapshots: bring the wanted commits in on a branch, re-apply the
rebranding, run the tests, then open a PR to `main`. Check licence notices (`NOTICE`,
`ATTRIBUTION.md`) when new assets or fonts arrive; fonts are never taken from proprietary sources.
