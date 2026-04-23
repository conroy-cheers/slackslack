# Live canary policy

Purpose:
- detect drift in stable public or higher-confidence private Slack interfaces
- keep live Slack checks out of the fast inner-loop and most CI runs

Policy:
- run live canaries only on scheduled or manually approved environments
- release readiness must not depend solely on live canaries
- canary failures must be classified as either:
  - `release_blocking`
  - `drift_triage`
- only interfaces marked `canary_eligible=true` in the coverage matrix may be used
  for live canaries
