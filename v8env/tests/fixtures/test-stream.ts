export function testStream(content: string, delay: 5) {
  return new ReadableStream({
    start(controller) {
      const encoder = new TextEncoder();
      const chunkSize = 1;
      let pos = 0;

      function push() {
        if (pos >= content.length) {
          controller.close();
          return;
        }

        controller.enqueue(
          encoder.encode(content.slice(pos, pos + chunkSize))
        );

        pos += chunkSize;

        setTimeout(push, delay);
      }
      push();
    }
  });
}
