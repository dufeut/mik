# hello-python - WASI HTTP component in Python using componentize-py
from wit_world import exports
from componentize_py_types import Ok
from wit_world.imports.types import (
    IncomingRequest, ResponseOutparam,
    OutgoingResponse, Fields, OutgoingBody
)

class IncomingHandler(exports.IncomingHandler):
    def handle(self, _: IncomingRequest, response_out: ResponseOutparam):
        # Construct the HTTP response with Content-Type header
        outgoingResponse = OutgoingResponse(Fields.from_list([
            ("content-type", b"application/json"),
        ]))
        outgoingResponse.set_status_code(200)

        # Get body handle
        outgoingBody = outgoingResponse.body()

        # Set response BEFORE writing body (WASI HTTP requirement)
        # Must use Ok() wrapper from componentize_py_types
        ResponseOutparam.set(response_out, Ok(outgoingResponse))

        # Write body
        outgoingBody.write().blocking_write_and_flush(
            b'{"message":"Hello from Python!","lang":"python"}'
        )
        OutgoingBody.finish(outgoingBody, None)
