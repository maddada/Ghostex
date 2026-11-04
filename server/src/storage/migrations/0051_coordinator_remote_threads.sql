-- CDXC:RemoteMachines 2026-11-03 WHY:
-- A coordinator's thread may run on another computer this one is connected to. Its link lives
-- here, on the coordinator's computer, keyed by that computer's server id as well, because a
-- (projectId, sessionId) pair only means something inside one gxserver's database and the other
-- computer may run an older gxserver that knows nothing of the coordinator. The columns after
-- `resolvedAt` are the remote supervisor's bookkeeping, as in `coordinator_threads`.
CREATE TABLE IF NOT EXISTS coordinator_remote_threads (
  serverId TEXT NOT NULL,
  projectId TEXT NOT NULL,
  sessionId TEXT NOT NULL,
  coordinatorProjectId TEXT NOT NULL,
  coordinatorSessionId TEXT NOT NULL,
  task TEXT NOT NULL DEFAULT '',
  title TEXT NOT NULL DEFAULT '',
  resolvedAt TEXT,
  observedWorking INTEGER NOT NULL DEFAULT 0 CHECK (observedWorking IN (0, 1)),
  reportedAt TEXT,
  reportedPromptKey TEXT,
  lastReport TEXT,
  lastState TEXT,
  createdAt TEXT NOT NULL,
  updatedAt TEXT NOT NULL,
  PRIMARY KEY (serverId, projectId, sessionId)
);

CREATE INDEX IF NOT EXISTS idx_coordinator_remote_threads_coordinator
  ON coordinator_remote_threads(coordinatorProjectId, coordinatorSessionId);

PRAGMA user_version = 51;
