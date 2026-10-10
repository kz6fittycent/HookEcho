# CI/CD notes carried over from Desktop-Scriptures

Every item below is a real failure documented in the comments of `/home/james/git/Desktop-Scriptures/.github/workflows/` (read in full on 2026-10-09). Hook Echo's workflows must keep each of them. The workflow files repeat the reason in a comment at the point where each fix is applied.

| Where | Rule | Why |
|---|---|---|
| All snap builds | `snapcore/action-build@v1` with **`snapcraft-args: pack`** | The input was renamed. `args` is silently ignored and runs bare `snapcraft` |
| Upload / promote | Channel **`candidate`** / **`stable`**, never `latest/...` | The `snapcraft export-login` macaroon grants bare risk levels; the track-qualified form fails with `invalid-channel-permission` |
| call-for-testing | Triggered by `workflow_run` on "🧪 Snap Builds" success on `main` | PR builds publish nothing, and a failed main build publishes nothing |
| call-for-testing | Rebuilds at `workflow_run.head_sha` | The build workflow publishes no artifact |
| promote-to-stable | Exact regex `^/promote ([0-9]+(,[0-9]+)*) stable( done)?[[:space:]]*$`; OWNER/MEMBER/COLLABORATOR; `environment: promote-stable`; 👍 reaction | Copy-paste from the issue always works and anything else fails loudly. Trailing newlines from copied code blocks are tolerated |
| ghvmctl screenshots | `snap install ghvmctl` retried 3× with a 30 s sleep | Transient Snap Store CDN 502 (core22) |
| ghvmctl screenshots | `ghvmctl exec "sudo snap install <snap> --revision N"`, not `ghvmctl snap-install` | `snap-install` always adds `--classic`, which strict snaps reject |
| ghvmctl screenshots | `sleep 60` after `snap-run` | `gnome-screenshot` succeeds before the window maps, so it isn't a readiness signal |
| screenshots branch | Push retried 3× with fetch + rebase between attempts | Transient GitHub 500s and concurrent pushes |
| screenshots | `if: failure()` step comments on the issue | Otherwise the issue says "screenshots coming shortly" forever |
| security-rebuild | Rebuild from the **release commit** (newest commit whose version file has exactly the stable version) | `main` may hold unreleased work |
| security-rebuild | `dry_run` input defaults to **true**; `force` input; skip while an open `security-rebuild` issue exists; `concurrency: security-rebuild` | Avoids daily duplicate rebuilds |
| security-rebuild | `review-tools` install retried | CDN flakiness |
| desktop-builds | Tag must equal `v<version>`, or the run fails | Prevents mislabeled releases |
| desktop-builds | Smoke test checks that config/cache dirs were created | Proves first-run paths work |
| desktop-builds | Native screenshots first, then a fallback renderer with a `::warning::` | CI machines sometimes lack a working native display path |
| desktop-builds | Windows: silent install test (`/VERYSILENT`) and check of the install path. macOS: ad-hoc `codesign --sign -` | Apple Silicon refuses unsigned binaries |
| desktop-builds | Only the 3 newest `test-v*` pre-releases are kept; `desktop-testing` issue; sha256 files | Storage, plus testers without GitHub accounts |
| data refresh | Start from the published release copy if it's newer than the repo's | The publish can run ahead of an unmerged PR |
| data refresh | Refuse to publish (fail the run) if entries were lost against the baseline | Never publish a regression |
| data refresh | `create-pull-request` with `add-paths` limited to the data files, then `gh pr merge --squash` | Nothing else rides along. A GITHUB_TOKEN merge doesn't trigger other workflows, so no candidate is built |
| data refresh | `gh release view X || gh release create X || gh release view X` | The release may be created concurrently by another run |

Translation for Hook Echo:

- The version comes from `[workspace.package] version` in `Cargo.toml`, not from `__init__.py`.
- PyInstaller becomes `cargo build --release`.
- Qt's offscreen fallback becomes our `--screenshots` path. It is already offscreen (wgpu, no window), so the "native first" step tries the default adapter, then forces the GL backend (`WGPU_BACKEND=gl`) before failing.
