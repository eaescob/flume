# Lessons

## Release: never pre-create the GitHub Release

The tag-push workflow (`.github/workflows/release.yml`) already creates the
GitHub Release with `softprops/action-gh-release@v2`. If a release for the
tag already exists when the workflow runs, the action fails with:

> Validation Failed: target_commitish cannot be changed when release is
> immutable

GitHub also marks releases immutable shortly after creation, so the
release can't be edited or have assets uploaded after the fact — recovery
requires deleting the release and re-running the workflow.

**Rule:** when releasing, push the tag and **stop**. Do not run
`gh release create`. The workflow handles release creation, binary
attachment, and downstream publish (Homebrew, AUR, packages, FreeBSD).
