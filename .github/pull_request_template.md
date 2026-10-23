## What and why

<!-- What does this change, and why? Link issues. -->

## Checklist

- [ ] New or changed actions have the right risk level (anything that changes the system is not `Read`)
- [ ] Arguments that reach commands or D-Bus are validated in `yaist-core/src/validate.rs`
- [ ] Works on Leap and Tumbleweed (and transactional systems if packages are involved)
- [ ] New dependencies: `scripts/license-report.sh` output checked, `docs/LICENSING.md` updated if the licence set changed
- [ ] Tested by hand: which commands or requests did you try?
