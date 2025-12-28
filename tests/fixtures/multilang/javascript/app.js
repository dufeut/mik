import {
  ResponseOutparam,
  OutgoingBody,
  OutgoingResponse,
  Fields,
} from 'wasi:http/types@0.2.0';

export const incomingHandler = {
  handle(_incomingRequest, responseOutparam) {
    // Start building an outgoing response
    const outgoingResponse = new OutgoingResponse(new Fields());

    // Access the outgoing response body
    let outgoingBody = outgoingResponse.body();
    {
      // Create a stream for the response body
      let outputStream = outgoingBody.write();
      // Write response
      outputStream.blockingWriteAndFlush(
        new Uint8Array(new TextEncoder().encode('{"message":"Hello from JavaScript!","lang":"javascript"}'))
      );
      // Dispose the stream
      outputStream[Symbol.dispose]();
    }

    // Set the status code for the response
    outgoingResponse.setStatusCode(200);
    // Finish the response body
    OutgoingBody.finish(outgoingBody, undefined);
    // Set the created response
    ResponseOutparam.set(responseOutparam, { tag: 'ok', val: outgoingResponse });
  }
};
