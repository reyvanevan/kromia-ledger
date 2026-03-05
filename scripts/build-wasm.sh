#!/usr/bin/env bash
set -euo pipefail

# ─── WASM Dual Build ────────────────────────────────────────────────
#
# Builds two WASM packages:
#   1. pkg/          — SIMD-enabled (default, .cargo/config.toml)
#   2. pkg-nosimd/   — Fallback for older browsers (Safari <16.4, etc.)
#
# The dashboard picks the right binary at runtime using:
#   WebAssembly.validate(simdTestBytes)
#
# Usage:
#   ./scripts/build-wasm.sh              # build both
#   ./scripts/build-wasm.sh --simd-only  # SIMD only (faster)
#   ./scripts/build-wasm.sh --help
#
# Browser SIMD support (as of 2025):
#   Chrome 91+, Firefox 89+, Safari 16.4+, Node 16.4+
#   ≈ 96% global coverage
# ────────────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

SIMD_ONLY=false

for arg in "$@"; do
    case $arg in
        --simd-only) SIMD_ONLY=true ;;
        --help|-h)
            echo "Usage: $0 [--simd-only] [--help]"
            echo ""
            echo "  --simd-only   Build only the SIMD-enabled package (faster)"
            echo "  --help        Show this help"
            exit 0
            ;;
        *)
            echo -e "${RED}Unknown option: $arg${NC}"
            exit 1
            ;;
    esac
done

# ── 1. SIMD Build (default — uses .cargo/config.toml) ───────────────

echo -e "${CYAN}━━━ Building SIMD-enabled WASM ━━━${NC}"
START=$(date +%s%N)

wasm-pack build --target web --out-dir pkg 2>&1

END=$(date +%s%N)
SIMD_TIME=$(( (END - START) / 1000000 ))
SIMD_SIZE=$(stat --printf="%s" pkg/kromia_ledger_bg.wasm 2>/dev/null || stat -f%z pkg/kromia_ledger_bg.wasm)
SIMD_SIZE_KB=$(( SIMD_SIZE / 1024 ))

echo -e "${GREEN}✅ SIMD build: ${SIMD_SIZE_KB}K in ${SIMD_TIME}ms → pkg/${NC}"

if $SIMD_ONLY; then
    echo ""
    echo -e "${GREEN}━━━ Done (SIMD only) ━━━${NC}"
    echo "  pkg/kromia_ledger_bg.wasm  ${SIMD_SIZE_KB}K (SIMD)"
    exit 0
fi

# ── 2. Non-SIMD Fallback Build ──────────────────────────────────────

echo ""
echo -e "${CYAN}━━━ Building non-SIMD fallback WASM ━━━${NC}"
START=$(date +%s%N)

# Override .cargo/config.toml by explicitly setting empty RUSTFLAGS
RUSTFLAGS="" wasm-pack build --target web --out-dir pkg-nosimd 2>&1

END=$(date +%s%N)
NOSIMD_TIME=$(( (END - START) / 1000000 ))
NOSIMD_SIZE=$(stat --printf="%s" pkg-nosimd/kromia_ledger_bg.wasm 2>/dev/null || stat -f%z pkg-nosimd/kromia_ledger_bg.wasm)
NOSIMD_SIZE_KB=$(( NOSIMD_SIZE / 1024 ))

echo -e "${GREEN}✅ Non-SIMD build: ${NOSIMD_SIZE_KB}K in ${NOSIMD_TIME}ms → pkg-nosimd/${NC}"

# ── Summary ──────────────────────────────────────────────────────────

echo ""
echo -e "${CYAN}━━━ Build Summary ━━━${NC}"
echo "  pkg/kromia_ledger_bg.wasm          ${SIMD_SIZE_KB}K  (SIMD — Chrome 91+, FF 89+, Safari 16.4+)"
echo "  pkg-nosimd/kromia_ledger_bg.wasm   ${NOSIMD_SIZE_KB}K  (fallback — all browsers)"
DELTA=$(( SIMD_SIZE_KB - NOSIMD_SIZE_KB ))
echo "  Size delta: +${DELTA}K for SIMD"
echo ""
echo -e "${YELLOW}Dashboard integration:${NC}"
echo '  // In your JS loader:'
echo '  const simdSupported = WebAssembly.validate(new Uint8Array(['
echo '    0,97,115,109,1,0,0,0,1,5,1,96,0,1,123,3,2,1,0,10,10,1,8,0,65,0,253,15,253,98,11'
echo '  ]));'
echo '  const wasmPath = simdSupported'
echo '    ? "/wasm/kromia_ledger_bg.wasm"'
echo '    : "/wasm-nosimd/kromia_ledger_bg.wasm";'
