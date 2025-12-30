// hello-typescript - A WASI HTTP handler in TypeScript
import {
  ResponseOutparam,
  OutgoingBody,
  OutgoingResponse,
  Fields,
} from "wasi:http/types@0.2.0";

export const incomingHandler = {
  handle(
    _incomingRequest: unknown,
    responseOutparam: ResponseOutparam
  ): void {
    const response = new OutgoingResponse(new Fields());
    response.setStatusCode(200);

    const body = response.body();
    const stream = body.write();
    stream.blockingWriteAndFlush(
      new Uint8Array(
        new TextEncoder().encode(
          JSON.stringify({
            message: "Hello from TypeScript!",
            service: "hello-typescript",
            lang: "typescript",
          })
        )
      )
    );
    stream[Symbol.dispose]();
    OutgoingBody.finish(body, undefined);
    ResponseOutparam.set(responseOutparam, { tag: "ok", val: response });
  },
};
