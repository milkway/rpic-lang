## R CMD check results

0 errors | 0 warnings | 1 note

* This is a new submission.
* The package contains compiled Rust code (via the extendr framework). Rust
  dependencies are vendored into `src/rust/vendor.tar.xz` and built offline, per
  the CRAN policy for Rust-based packages. `SystemRequirements: Cargo (Rust's
  package manager), rustc` is declared.

## Test environments

* local macOS (R 4.6), GitHub Actions: ubuntu-latest and macOS-latest.

## Notes for submission

CRAN submission is a manual step (https://cran.r-project.org/submit.html). Before
submitting:

1. Run `rextendr::vendor_pkgs("bindings/r")` so the vendored sources ship in the
   tarball.
2. `R CMD build bindings/r` then `R CMD check --as-cran rpic_*.tar.gz`.
3. Address any remaining NOTEs and submit the resulting tarball.
