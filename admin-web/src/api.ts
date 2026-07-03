const TOKEN_KEY = "himqtt_admin_token";
const USER_KEY = "himqtt_admin_user";

export function getToken() {
  return localStorage.getItem(TOKEN_KEY);
}

export function setSession(token: string, username: string) {
  localStorage.setItem(TOKEN_KEY, token);
  localStorage.setItem(USER_KEY, username);
}

export function clearSession() {
  localStorage.removeItem(TOKEN_KEY);
  localStorage.removeItem(USER_KEY);
}

export function getUsername() {
  return localStorage.getItem(USER_KEY) ?? "";
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const headers = new Headers(init.headers);
  headers.set("Content-Type", "application/json");
  const token = getToken();
  if (token) {
    headers.set("Authorization", `Bearer ${token}`);
  }

  const resp = await fetch(path, { ...init, headers });
  if (resp.status === 401) {
    clearSession();
    throw new Error("未授权，请重新登录");
  }
  if (!resp.ok) {
    const text = await resp.text();
    throw new Error(text || resp.statusText);
  }
  if (resp.status === 204) {
    return undefined as T;
  }
  return resp.json() as Promise<T>;
}

export const api = {
  login(username: string, password: string) {
    return request<{ token: string; username: string }>("/api/auth/login", {
      method: "POST",
      body: JSON.stringify({ username, password }),
    });
  },
  logout() {
    return request<void>("/api/auth/logout", { method: "POST" });
  },
  listMqttUsers() {
    return request<MqttUser[]>("/api/mqtt-users");
  },
  createMqttUser(body: { username: string; password: string; enabled: boolean }) {
    return request<MqttUser>("/api/mqtt-users", {
      method: "POST",
      body: JSON.stringify(body),
    });
  },
  updateMqttUser(id: number, body: { password?: string; enabled: boolean }) {
    return request<MqttUser>(`/api/mqtt-users/${id}`, {
      method: "PUT",
      body: JSON.stringify(body),
    });
  },
  deleteMqttUser(id: number) {
    return request<void>(`/api/mqtt-users/${id}`, { method: "DELETE" });
  },
  listTopics() {
    return request<MqttTopic[]>("/api/topics");
  },
  createTopic(body: { topic: string; description: string }) {
    return request<MqttTopic>("/api/topics", {
      method: "POST",
      body: JSON.stringify(body),
    });
  },
  updateTopic(id: number, body: { topic: string; description: string }) {
    return request<MqttTopic>(`/api/topics/${id}`, {
      method: "PUT",
      body: JSON.stringify(body),
    });
  },
  deleteTopic(id: number) {
    return request<void>(`/api/topics/${id}`, { method: "DELETE" });
  },
  listAcls() {
    return request<AclRule[]>("/api/acls");
  },
  createAcl(body: CreateAclBody) {
    return request<AclRule>("/api/acls", {
      method: "POST",
      body: JSON.stringify(body),
    });
  },
  updateAcl(id: number, body: UpdateAclBody) {
    return request<AclRule>(`/api/acls/${id}`, {
      method: "PUT",
      body: JSON.stringify(body),
    });
  },
  deleteAcl(id: number) {
    return request<void>(`/api/acls/${id}`, { method: "DELETE" });
  },
  monitorStats() {
    return request<MonitorStats>("/api/monitor/stats");
  },
  monitorMessages(params?: { recent?: boolean; limit?: number }) {
    const qs = new URLSearchParams();
    if (params?.recent) qs.set("recent", "true");
    if (params?.limit) qs.set("limit", String(params.limit));
    const suffix = qs.toString() ? `?${qs.toString()}` : "";
    return request<MessageRecord[]>(`/api/monitor/messages${suffix}`);
  },
  monitorConnections() {
    return request<ConnectionInfo[]>("/api/monitor/connections");
  },
  monitorSubscriptions() {
    return request<SubscriptionInfo[]>("/api/monitor/subscriptions");
  },
};

export type MqttUser = {
  id: number;
  username: string;
  enabled: boolean;
  created_at: string;
  updated_at: string;
};

export type MqttTopic = {
  id: number;
  topic: string;
  description: string;
  created_at: string;
};

export type AclRule = {
  id: number;
  username: string;
  topic_pattern: string;
  can_subscribe: boolean;
  can_publish: boolean;
  created_at: string;
};

export type CreateAclBody = {
  username: string;
  topic_pattern: string;
  can_subscribe: boolean;
  can_publish: boolean;
};

export type UpdateAclBody = {
  topic_pattern: string;
  can_subscribe: boolean;
  can_publish: boolean;
};

export type MonitorStats = {
  total_connections: number;
  total_subscriptions: number;
  total_messages: number;
  messages_last_minute: number;
  top_topics: { topic: string; count: number }[];
  updated_at: number;
};

export type MessageRecord = {
  id: number;
  ts: number;
  topic: string;
  payload: string;
  payload_bytes: number;
  qos: number;
  retain: boolean;
};

export type ConnectionInfo = {
  connection_id: number;
  client_id: string;
  username: string | null;
  clean: boolean;
  subscriptions: string[];
};

export type SubscriptionInfo = {
  filter: string;
  subscribers: string[];
};
