// Transform script - modifies input
export default function(input) {
    return {
        original: input.value,
        doubled: input.value * 2,
        squared: input.value * input.value,
        message: input.name + " processed"
    };
}
