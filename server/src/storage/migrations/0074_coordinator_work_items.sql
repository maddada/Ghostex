-- CDXC:Coordinators 2026-11-10 DECISION:
-- User, on the work items design page (Q2092 "B. Work items, with live cards under the reply",
-- Q2096 "replace"): a coordinator tracks each line of work as one item it updates with
-- `ghostex coordinator item …`, and work items replace the board; open board items move over once.
-- One row per line of work (server/src/coordinators/work.rs). `status` is the coordinator's own
-- word (later, needs, blocked, moving, waiting, finished); what it shows is worked out from that,
-- its open asks, its waits and its threads. Threads, asks, waits, steps and history are small JSON
-- lists on the row. `forThread` marks an item that exists only to place or hide one thread (a
-- board pin, `board move <thread> …`). `itemNumber` is the item's W<number>, the same number the
-- board item had as B<number>, and is never reused within a coordinator.
CREATE TABLE IF NOT EXISTS coordinator_work_items (
  coordinatorProjectId TEXT NOT NULL,
  coordinatorSessionId TEXT NOT NULL,
  itemNumber INTEGER NOT NULL,
  title TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'later',
  reason TEXT NOT NULL DEFAULT '',
  nowLine TEXT NOT NULL DEFAULT '',
  link TEXT NOT NULL DEFAULT '',
  release TEXT NOT NULL DEFAULT '',
  untested INTEGER NOT NULL DEFAULT 0 CHECK (untested IN (0, 1)),
  tag TEXT NOT NULL DEFAULT '',
  forThread INTEGER NOT NULL DEFAULT 0 CHECK (forThread IN (0, 1)),
  threadsJson TEXT NOT NULL DEFAULT '[]',
  asksJson TEXT NOT NULL DEFAULT '[]',
  waitsJson TEXT NOT NULL DEFAULT '[]',
  stepsJson TEXT NOT NULL DEFAULT '[]',
  historyJson TEXT NOT NULL DEFAULT '[]',
  createdAt TEXT NOT NULL,
  updatedAt TEXT NOT NULL,
  finishedAt TEXT,
  droppedAt TEXT,
  PRIMARY KEY (coordinatorProjectId, coordinatorSessionId, itemNumber)
);

-- The one-time move: every board item keeps its number (B7 becomes W7). Backlog is Later
-- (backlog), On hold is Waiting, Done is Finished and Removed is dropped; one-click answers become
-- the item's ask.
INSERT OR IGNORE INTO coordinator_work_items (
  coordinatorProjectId, coordinatorSessionId, itemNumber, title, status, reason, nowLine, link,
  release, untested, tag, forThread, threadsJson, asksJson, historyJson, createdAt, updatedAt,
  finishedAt, droppedAt
)
SELECT
  coordinatorProjectId,
  coordinatorSessionId,
  itemNumber,
  title,
  CASE
    WHEN doneAt IS NOT NULL OR boardGroup = 'done' THEN 'finished'
    WHEN boardGroup = 'hold' THEN 'waiting'
    WHEN boardGroup IN ('needs', 'blocked', 'moving') THEN boardGroup
    ELSE 'later'
  END,
  CASE
    WHEN boardGroup = 'ideas' THEN 'backlog'
    WHEN boardGroup = 'later' THEN reason
    ELSE ''
  END,
  note,
  link,
  release,
  untested,
  tag,
  0,
  '[]',
  CASE
    WHEN answers != '' THEN json_array(json_object(
      'tag', '', 'text', title, 'answers', answers, 'holds', json('false'), 'steps', '',
      'answered', answered, 'askedAt', createdAt,
      'settledAt', CASE
        WHEN answered != '' THEN updatedAt
        WHEN doneAt IS NOT NULL OR boardGroup = 'done' OR removedAt IS NOT NULL
          THEN COALESCE(doneAt, removedAt, updatedAt)
        ELSE NULL
      END,
      'withdrawn', CASE
        WHEN answered = '' AND (doneAt IS NOT NULL OR boardGroup = 'done' OR removedAt IS NOT NULL)
          THEN json('true')
        ELSE json('false')
      END
    ))
    ELSE '[]'
  END,
  json_array(json_object('at', updatedAt, 'kind', 'moved', 'text', 'From the board as B' || itemNumber, 'status',
    CASE
      WHEN doneAt IS NOT NULL OR boardGroup = 'done' THEN 'finished'
      WHEN boardGroup = 'hold' THEN 'waiting'
      WHEN boardGroup IN ('needs', 'blocked', 'moving') THEN boardGroup
      ELSE 'later'
    END)),
  createdAt,
  updatedAt,
  CASE WHEN doneAt IS NOT NULL OR boardGroup = 'done' THEN COALESCE(doneAt, updatedAt) END,
  removedAt
FROM coordinator_board_items
WHERE itemNumber > 0 AND boardGroup != 'dropped';

-- A thread's pin (a thread moved to a group, or hidden with ×) becomes an item for that thread,
-- numbered after the coordinator's highest item, titled as its session is.
INSERT OR IGNORE INTO coordinator_work_items (
  coordinatorProjectId, coordinatorSessionId, itemNumber, title, status, reason, nowLine, release,
  untested, forThread, threadsJson, historyJson, createdAt, updatedAt, droppedAt
)
SELECT
  pin.coordinatorProjectId,
  pin.coordinatorSessionId,
  COALESCE((
    SELECT MAX(item.itemNumber) FROM coordinator_board_items item
    WHERE item.coordinatorProjectId = pin.coordinatorProjectId
      AND item.coordinatorSessionId = pin.coordinatorSessionId
      AND item.itemNumber > 0
  ), 0) + ROW_NUMBER() OVER (
    PARTITION BY pin.coordinatorProjectId, pin.coordinatorSessionId ORDER BY pin.itemNumber DESC
  ),
  COALESCE(
    (SELECT session.title FROM sessions session
      WHERE session.projectId = pin.threadProjectId AND session.sessionId = pin.threadSessionId),
    (SELECT remote.title FROM coordinator_remote_threads remote
      WHERE remote.serverId = pin.threadServerId AND remote.projectId = pin.threadProjectId
        AND remote.sessionId = pin.threadSessionId),
    pin.threadSessionId
  ),
  CASE
    WHEN pin.boardGroup = 'hold' THEN 'waiting'
    WHEN pin.boardGroup IN ('needs', 'blocked', 'moving') THEN pin.boardGroup
    ELSE 'later'
  END,
  CASE WHEN pin.boardGroup = 'later' THEN pin.reason ELSE '' END,
  pin.note,
  pin.release,
  pin.untested,
  1,
  json_array(json_object(
    'serverId', pin.threadServerId, 'projectId', pin.threadProjectId,
    'sessionId', pin.threadSessionId
  )),
  json_array(json_object('at', pin.updatedAt, 'kind', 'moved', 'text', 'From the board', 'status',
    CASE
      WHEN pin.boardGroup = 'hold' THEN 'waiting'
      WHEN pin.boardGroup IN ('needs', 'blocked', 'moving') THEN pin.boardGroup
      ELSE 'later'
    END)),
  pin.createdAt,
  pin.updatedAt,
  pin.removedAt
FROM coordinator_board_items pin
WHERE pin.itemNumber < 0 AND pin.boardGroup NOT IN ('done', 'dropped')
  AND (pin.boardGroup != '' OR pin.removedAt IS NOT NULL);

PRAGMA user_version = 74;
