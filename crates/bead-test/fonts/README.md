# Test-only fonts

Never installed. Full IBM Plex Sans (OFL-1.1) and renamed FixtureSymbols subset
(Apache-2.0) are permissively licensed test inputs; retained licenses are adjacent.
No GPL code was copied into this crate. The harness maps application font families
to these files so host font discovery cannot change goldens. Font fallback and
real desktop font selection remain separately tested production concerns.

IBM Plex Sans upstream: google/fonts revision
bd8f81ddb5c74d5c8897b36ad88b440266245103, ofl/ibmplexsans.
SHA256 3b031aa4216174205bd8471f88a49b91f093169e9e87bd5262242bc5967fe2e3.

FixtureSymbols is a static default-axis subset named Bead Test Symbols from
Google Material Symbols Rounded (Apache-2.0), upstream
737e3324305806514d7909874fa1818ae1808232. Source SHA256
95b24392bb49efd1bc3e92cff4e2452ad094461bab7c97e7d8723fab97e330ca.
Capture: tools/build-test-font.py using isolated fonttools4.60.1; source hash is
verified, default variable axes instantiated, GSUB closure retained. This avoids
incorrect glyphs seen with the earlier unrelated renamed variable test subset.
No production fonts are changed. CI uses the committed bytes, not the Python tool.
Output SHA256 81aeebd29b992fee4a7f94b88e1feab968f0476bee9fa5850723c267b55efd83.
Capture validation: notification PNG has zero differing pixels against the full
pinned upstream font with the same fixed fixture, scale and CPU renderer.
