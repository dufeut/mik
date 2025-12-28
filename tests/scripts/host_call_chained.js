// Script that chains multiple WASM module calls
export default function(input) {
    // First call
    var first = host.call("echo", {
        method: "POST",
        path: "/first",
        body: { step: 1, data: input.data }
    });

    // Second call using first result
    var second = host.call("echo", {
        method: "POST",
        path: "/second",
        body: { step: 2, previous: first.body }
    });

    // Third call using second result
    var third = host.call("echo", {
        method: "POST",
        path: "/third",
        body: { step: 3, previous: second.body }
    });

    return {
        calls: 3,
        final_result: third.body
    };
}
