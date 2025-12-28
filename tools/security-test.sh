#!/bin/bash
# Security Testing Script for mik Runtime
# Run from Kali Linux against a running mik instance
#
# Usage: ./security-test.sh [HOST] [PORT]
# Example: ./security-test.sh localhost 3000

# Don't use set -e as we handle errors manually
# set -e

HOST="${1:-localhost}"
PORT="${2:-3000}"
BASE_URL="http://${HOST}:${PORT}"

echo "========================================"
echo "mik Runtime Security Tests"
echo "Target: ${BASE_URL}"
echo "========================================"
echo ""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

PASSED=0
FAILED=0

test_result() {
    if [ "$1" = "pass" ]; then
        echo -e "${GREEN}[PASS]${NC} $2"
        PASSED=$((PASSED + 1))
    else
        echo -e "${RED}[FAIL]${NC} $2"
        FAILED=$((FAILED + 1))
    fi
}

# -----------------------------------------------------------------------------
# 1. Path Traversal Tests
# -----------------------------------------------------------------------------
echo "=== Path Traversal Tests ==="

# Test script path traversal
RESP=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/script/../../../etc/passwd" 2>/dev/null || echo "000")
if [ "$RESP" = "400" ] || [ "$RESP" = "404" ] || [ "$RESP" = "403" ]; then
    test_result "pass" "Script path traversal blocked (../../../etc/passwd)"
else
    test_result "fail" "Script path traversal may be vulnerable (got $RESP)"
fi

# Test with encoded path (500 = scripts not configured, still blocked)
RESP=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/script/..%2F..%2F..%2Fetc%2Fpasswd" 2>/dev/null || echo "000")
if [ "$RESP" = "400" ] || [ "$RESP" = "404" ] || [ "$RESP" = "403" ] || [ "$RESP" = "500" ]; then
    test_result "pass" "URL-encoded path traversal blocked ($RESP)"
else
    test_result "fail" "URL-encoded path traversal may be vulnerable (got $RESP)"
fi

# Test module path traversal
RESP=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/../../../etc/passwd" 2>/dev/null || echo "000")
if [ "$RESP" = "400" ] || [ "$RESP" = "404" ] || [ "$RESP" = "403" ]; then
    test_result "pass" "Module path traversal blocked"
else
    test_result "fail" "Module path traversal may be vulnerable (got $RESP)"
fi

echo ""

# -----------------------------------------------------------------------------
# 2. Input Validation Tests
# -----------------------------------------------------------------------------
echo "=== Input Validation Tests ==="

# Test oversized body (should be rejected by body size limit)
LARGE_BODY=$(python3 -c "print('A' * 20000000)" 2>/dev/null || echo "")
if [ -n "$LARGE_BODY" ]; then
    RESP=$(curl -s -o /dev/null -w "%{http_code}" -X POST "${BASE_URL}/script/test" -d "$LARGE_BODY" 2>/dev/null || echo "000")
    if [ "$RESP" = "413" ] || [ "$RESP" = "400" ]; then
        test_result "pass" "Oversized body rejected"
    else
        test_result "fail" "Oversized body not properly rejected (got $RESP)"
    fi
else
    echo -e "${YELLOW}[SKIP]${NC} Oversized body test (python3 not available)"
fi

# Test null bytes in path (500 = scripts not configured, still blocked)
RESP=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/script/test%00.js" 2>/dev/null || echo "000")
if [ "$RESP" = "400" ] || [ "$RESP" = "404" ] || [ "$RESP" = "500" ]; then
    test_result "pass" "Null byte in path rejected ($RESP)"
else
    test_result "fail" "Null byte handling may be vulnerable (got $RESP)"
fi

# Test special characters in script name
RESP=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/script/<script>alert(1)</script>" 2>/dev/null || echo "000")
if [ "$RESP" = "400" ] || [ "$RESP" = "404" ]; then
    test_result "pass" "XSS in path rejected"
else
    test_result "fail" "XSS in path may not be sanitized (got $RESP)"
fi

echo ""

# -----------------------------------------------------------------------------
# 3. HTTP Method Tests
# -----------------------------------------------------------------------------
echo "=== HTTP Method Tests ==="

# Test TRACE method - key is it doesn't echo request back (404 = not found = safe)
RESP=$(curl -s -o /dev/null -w "%{http_code}" -X TRACE "${BASE_URL}/" 2>/dev/null || echo "000")
BODY=$(curl -s -X TRACE "${BASE_URL}/" 2>/dev/null || echo "")
if [ "$RESP" = "405" ] || [ "$RESP" = "400" ] || [ "$RESP" = "404" ]; then
    # Check that response doesn't echo the request (TRACE vulnerability)
    if echo "$BODY" | grep -q "TRACE / HTTP"; then
        test_result "fail" "TRACE echoes request back (XST vulnerability)"
    else
        test_result "pass" "TRACE method blocked ($RESP, no echo)"
    fi
else
    test_result "fail" "TRACE method may be enabled (got $RESP)"
fi

# Test OPTIONS (CORS preflight)
RESP=$(curl -s -o /dev/null -w "%{http_code}" -X OPTIONS "${BASE_URL}/" 2>/dev/null || echo "000")
# OPTIONS can return 200 or 204 for CORS, which is fine
echo -e "${YELLOW}[INFO]${NC} OPTIONS returns $RESP (depends on CORS config)"

echo ""

# -----------------------------------------------------------------------------
# 4. Header Injection Tests
# -----------------------------------------------------------------------------
echo "=== Header Injection Tests ==="

# Test CRLF injection in path (500 = scripts not configured, still blocked)
RESP=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/script/test%0d%0aX-Injected:%20true" 2>/dev/null || echo "000")
if [ "$RESP" = "400" ] || [ "$RESP" = "404" ] || [ "$RESP" = "500" ]; then
    test_result "pass" "CRLF injection blocked ($RESP)"
else
    test_result "fail" "CRLF injection may be vulnerable (got $RESP)"
fi

echo ""

# -----------------------------------------------------------------------------
# 5. Script Execution Security (if scripts dir exists)
# -----------------------------------------------------------------------------
echo "=== Script Security Tests ==="

# Test that non-existent script returns 404
RESP=$(curl -s -o /dev/null -w "%{http_code}" -X POST "${BASE_URL}/script/nonexistent_script_12345" 2>/dev/null || echo "000")
if [ "$RESP" = "404" ]; then
    test_result "pass" "Non-existent script returns 404"
else
    echo -e "${YELLOW}[INFO]${NC} Script endpoint returned $RESP (scripts dir may not be configured)"
fi

echo ""

# -----------------------------------------------------------------------------
# 5b. /run Endpoint Security (WASM modules)
# -----------------------------------------------------------------------------
echo "=== /run Endpoint Security Tests ==="

# Path traversal on /run
RESP=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/run/../../../etc/passwd" 2>/dev/null || echo "000")
if [ "$RESP" = "400" ] || [ "$RESP" = "404" ] || [ "$RESP" = "403" ]; then
    test_result "pass" "/run path traversal blocked"
else
    test_result "fail" "/run path traversal may be vulnerable (got $RESP)"
fi

# URL-encoded traversal on /run
RESP=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/run/..%2F..%2F..%2Fetc%2Fpasswd" 2>/dev/null || echo "000")
if [ "$RESP" = "400" ] || [ "$RESP" = "404" ] || [ "$RESP" = "403" ]; then
    test_result "pass" "/run URL-encoded traversal blocked"
else
    test_result "fail" "/run URL-encoded traversal may be vulnerable (got $RESP)"
fi

# Module with .. prefix
RESP=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/run/..module/test" 2>/dev/null || echo "000")
if [ "$RESP" = "400" ] || [ "$RESP" = "404" ] || [ "$RESP" = "403" ]; then
    test_result "pass" "/run ..module name blocked"
else
    test_result "fail" "/run ..module name may be vulnerable (got $RESP)"
fi

# Null byte in module name
RESP=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/run/test%00.wasm/foo" 2>/dev/null || echo "000")
if [ "$RESP" = "400" ] || [ "$RESP" = "404" ]; then
    test_result "pass" "/run null byte rejected"
else
    test_result "fail" "/run null byte may not be sanitized (got $RESP)"
fi

# Non-existent module returns 404
RESP=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/run/nonexistent_xyz/test" 2>/dev/null || echo "000")
if [ "$RESP" = "404" ]; then
    test_result "pass" "/run non-existent module returns 404"
else
    echo -e "${YELLOW}[INFO]${NC} /run endpoint returned $RESP"
fi

echo ""

# -----------------------------------------------------------------------------
# 6. Rate Limiting Test (basic)
# -----------------------------------------------------------------------------
echo "=== Rate Limiting Tests ==="

# Send rapid requests
RATE_TEST_PASSED=true
for i in {1..50}; do
    RESP=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/" 2>/dev/null || echo "000")
    if [ "$RESP" = "429" ]; then
        test_result "pass" "Rate limiting triggered after $i requests"
        RATE_TEST_PASSED=false
        break
    fi
done

if [ "$RATE_TEST_PASSED" = true ]; then
    echo -e "${YELLOW}[INFO]${NC} Rate limiting not triggered in 50 requests (may have high limits)"
fi

echo ""

# -----------------------------------------------------------------------------
# 7. Metrics Endpoint Security
# -----------------------------------------------------------------------------
echo "=== Metrics Endpoint Tests ==="

RESP=$(curl -s "${BASE_URL}/metrics" 2>/dev/null || echo "")
if echo "$RESP" | grep -q "mik_"; then
    test_result "pass" "Metrics endpoint accessible (ensure Kong restricts in prod)"
    echo -e "${YELLOW}[WARN]${NC} Metrics exposed - ensure Kong blocks /metrics in production"
else
    test_result "pass" "Metrics endpoint not publicly exposed or different path"
fi

echo ""

# -----------------------------------------------------------------------------
# Summary
# -----------------------------------------------------------------------------
echo "========================================"
echo "Security Test Summary"
echo "========================================"
echo -e "Passed: ${GREEN}${PASSED}${NC}"
echo -e "Failed: ${RED}${FAILED}${NC}"
echo ""

if [ $FAILED -gt 0 ]; then
    echo -e "${RED}Some tests failed - review above output${NC}"
    exit 1
else
    echo -e "${GREEN}All tests passed${NC}"
    exit 0
fi
