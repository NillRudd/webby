# Render References

`color-block.ppm` is a checked-in deterministic software-render reference. PPM
is intentionally used because it is dependency-free, easy to inspect, and
byte-for-byte reproducible in tests.

Run `scripts/demo.sh` to generate a broader local demo bundle under
`target/webby-demo/`.
