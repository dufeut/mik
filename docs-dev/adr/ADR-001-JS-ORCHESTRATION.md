# ADR-001: JavaScript Orchestration for Handler Composition

**Status:** Accepted
**Date:** 2025-12-27
**Decision Makers:** Project maintainer
**Context:** Multi-handler composition strategy for mik runtime

---

## Summary

Use JavaScript scripts (via rquickjs) for runtime orchestration of WASM handlers instead of Component Model static composition.

---

## Context

mik needs a way to compose multiple WASM handlers into workflows. For example, a checkout flow might need to:

1. Verify authentication
2. Validate inventory
3. Process payment
4. Create order
5. Send notifications

Each step is a separate WASM handler. We need a composition mechanism.

### Problem Statement

How should mik enable developers to chain multiple handlers together while maintaining:
- Security (no credential leakage, controlled network access)
- Flexibility (conditional logic, error handling, dynamic routing)
- Developer experience (low learning curve, fast iteration)
- Performance (acceptable overhead for HTTP workloads)

---

## Decision Drivers

1. **Runtime flexibility** - Need to make decisions based on response data
2. **Developer ergonomics** - Minimize learning curve for composition
3. **Security isolation** - Orchestration layer should not have network access
4. **Error handling** - Centralized place for retries, fallbacks, circuit breaking
5. **Debugging** - Easy to trace and log orchestration flow
6. **Iteration speed** - Change composition without recompiling

---

## Options Considered

### Option 1: Component Model Static Composition

Use WIT imports/exports to wire components together at build time.

```wit
// checkout.wit
interface checkout {
    use auth.{verify};
    use inventory.{check};
    use payment.{process};
    use orders.{create};

    checkout: func(request: incoming-request) -> outgoing-response;
}
```

Composition via `wac plug` at build time:
```bash
wac plug checkout.wasm --plug auth.wasm --plug inventory.wasm -o composed.wasm
```

**Pros:**
- Type-safe at compile time (WIT validates interfaces)
- No runtime overhead for dispatch
- Standard tooling (`wac`, `wasm-tools`)
- Portable across runtimes (Spin, wasmCloud)
- Components remain decoupled (interface-based)

**Cons:**
- All paths must be wired statically - no runtime branching
- Changing composition requires rebuild
- Must define WIT interfaces for internal composition
- Learning curve for WIT and composition tooling
- Cannot skip steps conditionally based on runtime data
- Error handling distributed across components

**Example limitation:**
```
// Cannot do this with static composition:
if (auth.status === 401 && input.allow_guest) {
    skip_to_guest_checkout();
}
```

### Option 2: JavaScript Runtime Orchestration (Selected)

Use a sandboxed JavaScript runtime (rquickjs) for orchestration.

```javascript
// scripts/checkout.js
export default function(input) {
    // Step 1: Auth (with conditional guest flow)
    var auth = host.call("auth", {
        path: "/verify",
        body: { token: input.token }
    });

    if (auth.status === 401 && input.allow_guest) {
        return host.call("guest-checkout", { body: input });
    }

    if (auth.status !== 200) {
        return { error: "Unauthorized", status: 401 };
    }

    // Step 2: Inventory check
    var inventory = host.call("inventory", {
        path: "/check",
        body: { items: input.items }
    });

    if (!inventory.body.available) {
        return {
            error: "Items unavailable",
            unavailable: inventory.body.missing,
            status: 422
        };
    }

    // Step 3: Payment (with retry logic)
    var payment;
    for (var attempt = 0; attempt < 3; attempt++) {
        payment = host.call("payment", {
            path: "/charge",
            body: { amount: inventory.body.total, method: input.payment }
        });
        if (payment.status === 200) break;
        if (payment.status !== 503) break; // Only retry on service unavailable
    }

    if (payment.status !== 200) {
        return { error: "Payment failed", status: 402 };
    }

    // Step 4: Create order
    var order = host.call("orders", {
        path: "/create",
        body: {
            userId: auth.body.userId,
            items: input.items,
            paymentId: payment.body.id
        }
    });

    // Step 5: Notifications (fire-and-forget, don't fail checkout)
    host.call("notifications", {
        path: "/send",
        body: {
            type: "order_confirmation",
            orderId: order.body.id,
            email: auth.body.email
        }
    });

    return {
        orderId: order.body.id,
        total: inventory.body.total,
        status: 201
    };
}
```

**Pros:**
- Runtime flexibility (conditional logic, dynamic routing)
- Familiar syntax for most developers
- Centralized error handling and retry logic
- No recompilation to change orchestration
- Easy to debug (single file, add console.log)
- Security: scripts cannot access network directly
- Can aggregate/transform responses

**Cons:**
- No compile-time type checking of handler calls
- JS runtime adds ~1,300 LoC to codebase
- Small performance overhead (JS interpretation)
- Scripts only work with mik (not portable)
- Must maintain async/sync bridge (rquickjs is sync)

### Option 3: Declarative YAML/TOML Workflows

Define workflows in configuration:

```yaml
# workflows/checkout.yaml
name: checkout
steps:
  - name: auth
    handler: auth
    path: /verify
    on_failure: abort

  - name: inventory
    handler: inventory
    path: /check

  - name: payment
    handler: payment
    path: /charge
    retry: 3

  - name: order
    handler: orders
    path: /create
    inputs:
      userId: "{{ auth.body.userId }}"
```

**Pros:**
- No code required for simple workflows
- Easy to validate and visualize
- Could generate to either JS or Component Model

**Cons:**
- Limited expressiveness (complex conditions become awkward)
- Yet another DSL to learn
- Template syntax for data flow is error-prone
- Eventually need escape hatch for complex logic

### Option 4: Rust Orchestration Layer

Write orchestration in Rust, compile to WASM:

```rust
// orchestrator.rs
fn checkout(input: Request) -> Response {
    let auth = call_handler("auth", "/verify", &input)?;
    if auth.status != 200 {
        return Response::error(401, "Unauthorized");
    }
    // ... etc
}
```

**Pros:**
- Type-safe
- Compiles to WASM (same as handlers)
- Full language power

**Cons:**
- Recompile for any orchestration change
- Higher barrier to entry than JS
- Orchestration logic mixed with handler code
- Rust async complexity for sequential calls

---

## Decision

**Selected: Option 2 - JavaScript Runtime Orchestration**

### Rationale

1. **Flexibility wins for orchestration**: Orchestration is inherently about conditional logic, error handling, and data flow. JS excels at this while Component Model is designed for static capability composition.

2. **Security through limitation**: By giving scripts NO network access (only `host.call()`), we create a clear security boundary. Scripts can only compose handlers, not bypass them.

3. **Iteration speed matters**: Changing a workflow shouldn't require recompilation. JS scripts can be edited and tested immediately.

4. **Familiar tooling**: Most developers know JS. Component Model composition requires learning WIT, `wac`, and composition semantics.

5. **Acceptable overhead**: For HTTP request/response workloads (not compute-intensive), JS interpretation overhead is negligible compared to network latency.

6. **Handlers remain portable**: The handlers themselves are standard WASM components. Only the orchestration scripts are mik-specific. If needed, handlers can run on Spin/wasmCloud with different composition.

---

## Consequences

### Positive

- Developers can write complex orchestration logic easily
- Scripts are sandboxed (security by design)
- Fast iteration (no recompile for workflow changes)
- Centralized error handling, retries, and fallbacks
- Can implement patterns like saga, circuit breaker at orchestration level
- Easy debugging with single-file visibility

### Negative

- No compile-time validation of handler calls
- Runtime errors if handler name is misspelled
- Scripts are mik-specific (vendor lock-in at orchestration layer)
- ~1,300 LoC maintenance burden for async/sync bridge
- Cannot use Component Model tooling for composition analysis

### Neutral

- Performance overhead is measurable but acceptable for HTTP workloads
- Different mental model than Component Model (neither better nor worse)

---

## Alternatives Not Selected

| Option | Reason Not Selected |
|--------|---------------------|
| Component Model | Too rigid for orchestration; no runtime branching; high learning curve |
| YAML Workflows | Limited expressiveness; would eventually need JS escape hatch anyway |
| Rust Orchestration | Recompile overhead; higher barrier to entry; overkill for glue code |

---

## Implementation Notes

### Security Model

Scripts execute in rquickjs with NO access to:
- Network (`fetch`, `XMLHttpRequest`)
- Filesystem
- Module imports (`require`, dynamic `import`)
- Process/shell
- Timers beyond execution timeout

Only available API:
- `input` - Request body (parsed JSON)
- `host.call(module, options)` - Call a WASM handler

### Async/Sync Bridge

rquickjs is a synchronous JavaScript runtime. Handlers are async (Tokio). The bridge (`runtime/script.rs`) uses:
- Thread-local storage for bridge state
- Channels for sync JS → async Rust communication
- RAII guards for cleanup on panic

### Handler Call Flow

```
JS: host.call("auth", {...})
    ↓
Bridge: Send message to async runtime
    ↓
Runtime: Load handler, execute WASI HTTP call
    ↓
Bridge: Block until response
    ↓
JS: Receive { status, headers, body }
```

---

## Related Decisions

- ADR-002: Sidecar Security Model (handlers access infra via HTTP to sidecars)
- ADR-003: Bridge Composition (mik:core/handler → wasi:http/incoming-handler)

---

## References

- [Component Model Composition](https://component-model.bytecodealliance.org/creating-and-consuming/composing.html)
- [rquickjs Documentation](https://github.com/DelSkaorth/rquickjs)
- [Spin Component Dependencies](https://developer.fermyon.com/spin/v3/component-dependencies)
- [wasmCloud wRPC](https://wasmcloud.com/docs/concepts/lattice)

---

## Changelog

| Date | Change |
|------|--------|
| 2025-12-27 | Initial decision documented |
