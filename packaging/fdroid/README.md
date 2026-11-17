# F-Droid

`io.github.rajatmo.mathcas.yml` is the build recipe submitted to F-Droid's data repository
(`fdroid/fdroiddata` on GitLab, as `metadata/io.github.rajatmo.mathcas.yml`). F-Droid builds each release tag
from source and signs the package with its own key: `UNSIGNED=1 packaging/android/build-apk.sh` leaves the
package unsigned for that. A package from F-Droid and one from the project's releases are signed with different
keys, so one cannot update the other.

The store's texts and pictures are in `fastlane/metadata/android/en-US/` (title, descriptions, icon, phone
screenshots, and `changelogs/<version code>.txt` for each release; the version code of `a.b.c` is
`10000 a + 100 b + c`).
