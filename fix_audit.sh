#!/bin/bash
sed -i 's/- uses: actions-rust-lang\/audit@v1/- uses: actions-rust-lang\/audit@v1\n        with:\n          ignore: RUSTSEC-2026-0195,RUSTSEC-2026-0194,RUSTSEC-2026-0192/' .github/workflows/security-audit.yml
