import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type UUID = string;
export type Topology = "STANDALONE" | "CLUSTER" | "SENTINEL";

export interface InstanceCfg {
  id: UUID;
  name: string;
  topology: Topology;
  use_tls: boolean;
  username?: string | null;
  password?: string | null;
  db?: number | null;
  host?: string | null;
  port?: number | null;
  cluster_nodes: string[];
  sentinel_master_id?: string | null;
  sentinel_nodes: string[];
  timeout_ms?: number | null;
}

export const api = {
  listInstances: () => invoke<InstanceCfg[]>("list_instances"),
  newInstance: () => invoke<InstanceCfg>("new_instance"),
  saveInstance: (cfg: InstanceCfg) => invoke("save_instance", { cfg }),
  removeInstance: (id: UUID) => invoke("remove_instance", { id }),
  connect: (id: UUID) => invoke("connect_instance", { id }),
  disconnect: (id: UUID) => invoke("disconnect_instance", { id }),

  scan: (id: UUID, pattern: string, cursor?: number, count?: number) =>
    invoke<[number, string[]]>("scan_keys", { id, pattern, cursor, count }),

  keyType: (id: UUID, key: string) => invoke<string>("key_type", { id, key }),
  getTTL: (id: UUID, key: string) => invoke<number>("get_ttl", { id, key }),
  setTTL: (id: UUID, key: string, ttlSeconds?: number | null) =>
    invoke<boolean>("set_ttl", { id, key, ttlSeconds }),

  renameKey: (id: UUID, oldKey: string, newKey: string) =>
    invoke<boolean>("rename_key", { id, oldKey, newKey }),
  delKey: (id: UUID, key: string) => invoke<number>("del_key", { id, key }),

  getString: (id: UUID, key: string) =>
    invoke<string | null>("get_string", { id, key }),
  setString: (
    id: UUID,
    key: string,
    value: string,
    ttlSeconds?: number | null,
  ) => invoke("set_string", { id, key, value, ttlSeconds }),

  listRange: (id: UUID, key: string, start: number, stop: number) =>
    invoke<string[]>("list_range", { id, key, start, stop }),
  listPush: (id: UUID, key: string, left: boolean, values: string[]) =>
    invoke<number>("list_push", { id, key, left, values }),
  listRemove: (id: UUID, key: string, value: string, count: number) =>
    invoke<number>("list_remove", { id, key, value, count }),

  setMembers: (id: UUID, key: string, limit: number) =>
    invoke<string[]>("set_members", { id, key, limit }),
  sadd: (id: UUID, key: string, members: string[]) =>
    invoke<number>("sadd", { id, key, members }),
  srem: (id: UUID, key: string, members: string[]) =>
    invoke<number>("srem", { id, key, members }),

  zrangeWithscores: (id: UUID, key: string, start: number, stop: number) =>
    invoke<[string, number][]>("zrange_withscores", { id, key, start, stop }),
  zadd: (id: UUID, key: string, member: string, score: number) =>
    invoke<number>("zadd", { id, key, member, score }),
  zrem: (id: UUID, key: string, member: string) =>
    invoke<number>("zrem", { id, key, member }),
  zincrby: (id: UUID, key: string, member: string, by: number) =>
    invoke<number>("zincrby", { id, key, member, by }),

  hgetall: (id: UUID, key: string) =>
    invoke<[string, string][]>("hgetall", { id, key }),
  hset: (id: UUID, key: string, field: string, value: string) =>
    invoke<number>("hset", { id, key, field, value }),
  hdel: (id: UUID, key: string, field: string) =>
    invoke<number>("hdel", { id, key, field }),

  xadd: (id: UUID, key: string, fields: [string, string][]) =>
    invoke<string>("xadd", { id, key, fields }),

  subscribe: (id: UUID, channel: string) =>
    invoke<UUID>("subscribe", { id, channel }),
  psubscribe: (id: UUID, pattern: string) =>
    invoke<UUID>("psubscribe", { id, pattern }),
  unsubscribe: (id: UUID, subId: UUID) => invoke("unsubscribe", { id, subId }),
  publish: (id: UUID, channel: string, message: string) =>
    invoke<number>("publish", { id, channel, message }),

  onPubSubMessage: (
    cb: (payload: { channel: string; message: string }) => void,
  ) => listen("pubsub://message", (e) => cb(e.payload as any)),
};
