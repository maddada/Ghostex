import Darwin
import Foundation

final class ClientConnection {
  let id = UUID()

  private weak var daemon: EditorDaemon?
  private let readQueue: DispatchQueue
  private let writeQueue: DispatchQueue
  private var fileDescriptor: Int32
  private var readSource: DispatchSourceRead?
  private var buffer = Data()
  private let lock = NSLock()
  private var closed = false

  init(fileDescriptor: Int32, daemon: EditorDaemon, readQueue: DispatchQueue) {
    self.fileDescriptor = fileDescriptor
    self.daemon = daemon
    self.readQueue = readQueue
    self.writeQueue = DispatchQueue(label: "com.madda.ghostex.editor.connection.\(id.uuidString).write")
  }

  func start() {
    do {
      disableSigPipe(fileDescriptor)
      try setCloseOnExec(fileDescriptor)
      try setNonBlocking(fileDescriptor)
    } catch {
      close()
      return
    }

    let source = DispatchSource.makeReadSource(fileDescriptor: fileDescriptor, queue: readQueue)
    readSource = source
    source.setEventHandler { [weak self] in
      self?.readAvailableBytes()
    }
    source.resume()
  }

  func send(_ object: [String: Any], completion: (() -> Void)? = nil) {
    guard let line = jsonLineData(object) else {
      completion?()
      return
    }

    lock.lock()
    let targetFileDescriptor = closed ? -1 : fileDescriptor
    lock.unlock()

    guard targetFileDescriptor >= 0 else {
      completion?()
      return
    }

    writeQueue.async {
      _ = writeAll(line, to: targetFileDescriptor)
      completion?()
    }
  }

  func sendError(_ message: String) {
    send(["type": "error", "v": ghostexEditorProtocolVersion, "message": message])
  }

  func close() {
    lock.lock()
    if closed {
      lock.unlock()
      return
    }
    closed = true
    let source = readSource
    readSource = nil
    let descriptorToClose = fileDescriptor
    fileDescriptor = -1
    lock.unlock()

    source?.cancel()
    if descriptorToClose >= 0 {
      Darwin.close(descriptorToClose)
    }

    DispatchQueue.main.async { [weak self] in
      guard let self else {
        return
      }
      self.daemon?.connectionClosed(self)
    }
  }

  private func readAvailableBytes() {
    while true {
      var bytes = [UInt8](repeating: 0, count: 4096)
      let byteCapacity = bytes.count
      let count = bytes.withUnsafeMutableBytes { rawBuffer in
        Darwin.read(fileDescriptor, rawBuffer.baseAddress, byteCapacity)
      }

      if count > 0 {
        buffer.append(contentsOf: bytes.prefix(count))
        processBufferedLines()
        continue
      }

      if count == 0 {
        close()
        return
      }

      if errno == EINTR {
        continue
      }
      if errno == EAGAIN || errno == EWOULDBLOCK {
        return
      }

      close()
      return
    }
  }

  private func processBufferedLines() {
    while let newlineIndex = buffer.firstIndex(of: 0x0A) {
      let line = buffer.prefix(upTo: newlineIndex)
      buffer.removeSubrange(buffer.startIndex...newlineIndex)
      handleLine(Data(line))
    }
  }

  private func handleLine(_ line: Data) {
    let trimmedLine: Data
    if line.last == 0x0D {
      trimmedLine = line.dropLast()
    } else {
      trimmedLine = line
    }

    guard !trimmedLine.isEmpty else {
      sendError("malformed JSON")
      return
    }

    do {
      guard let object = try JSONSerialization.jsonObject(with: trimmedLine) as? [String: Any] else {
        sendError("request must be a JSON object")
        return
      }
      DispatchQueue.main.async { [weak self] in
        guard let self else {
          return
        }
        self.daemon?.handleRequest(object, from: self)
      }
    } catch {
      sendError("malformed JSON")
    }
  }
}
