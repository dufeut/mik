// hello-c - WASI HTTP component in C using wasi-sdk
#include "gen/proxy.h"
#include <string.h>

static const char* RESPONSE_BODY = "{\"message\":\"Hello from C!\",\"lang\":\"c\"}";

void exports_wasi_http_incoming_handler_handle(
    exports_wasi_http_incoming_handler_own_incoming_request_t request,
    exports_wasi_http_incoming_handler_own_response_outparam_t response_out
) {
    // Drop the request (we don't need it for this simple example)
    wasi_http_types_incoming_request_drop_own(request);

    // Create response headers
    wasi_http_types_own_fields_t headers = wasi_http_types_constructor_fields();
    wasi_http_types_own_outgoing_response_t response =
        wasi_http_types_constructor_outgoing_response(headers);

    // Set status code
    wasi_http_types_borrow_outgoing_response_t resp_borrow =
        (wasi_http_types_borrow_outgoing_response_t){ response.__handle };
    wasi_http_types_method_outgoing_response_set_status_code(resp_borrow, 200);

    // Get body and stream
    wasi_http_types_own_outgoing_body_t body;
    wasi_http_types_method_outgoing_response_body(resp_borrow, &body);

    wasi_io_streams_own_output_stream_t stream;
    wasi_http_types_borrow_outgoing_body_t body_borrow =
        (wasi_http_types_borrow_outgoing_body_t){ body.__handle };
    wasi_http_types_method_outgoing_body_write(body_borrow, &stream);

    // Set response BEFORE writing body (WASI HTTP requirement)
    wasi_http_types_result_own_outgoing_response_error_code_t result = {
        .is_err = false, .val.ok = response
    };
    wasi_http_types_static_response_outparam_set(response_out, &result);

    // Write body
    proxy_list_u8_t body_bytes = { (uint8_t*)RESPONSE_BODY, strlen(RESPONSE_BODY) };
    wasi_io_streams_stream_error_t err;
    wasi_io_streams_borrow_output_stream_t stream_borrow =
        (wasi_io_streams_borrow_output_stream_t){ stream.__handle };
    wasi_io_streams_method_output_stream_blocking_write_and_flush(stream_borrow, &body_bytes, &err);

    // Cleanup
    wasi_io_streams_output_stream_drop_own(stream);
    wasi_http_types_error_code_t body_err;
    wasi_http_types_static_outgoing_body_finish(body, NULL, &body_err);
}
