import { FormEvent, useEffect, useState } from "react";
import {
  api,
  AclRule,
  clearSession,
  CreateAclBody,
  getToken,
  getUsername,
  MqttTopic,
  MqttUser,
  setSession,
} from "./api";
import { AppLayout, NavKey } from "./components/AppLayout";
import { DetailModal } from "./components/DetailModal";
import { MonitorPage } from "./pages/MonitorPage";

export default function App() {
  const [token, setToken] = useState(getToken());
  const [username, setUsername] = useState(getUsername());
  const [tab, setTab] = useState<NavKey>("monitor");
  const [error, setError] = useState("");

  if (!token) {
    return (
      <LoginPage
        onLogin={(t, u) => {
          setSession(t, u);
          setToken(t);
          setUsername(u);
        }}
      />
    );
  }

  async function logout() {
    try {
      await api.logout();
    } catch {
      // ignore
    }
    clearSession();
    setToken(null);
  }

  return (
    <AppLayout
      active={tab}
      username={username}
      onNavigate={(key) => {
        setTab(key);
        setError("");
      }}
      onLogout={logout}
    >
      {error && <div className="error-banner">{error}</div>}
      {tab === "monitor" && <MonitorPage />}
      {tab === "users" && <UsersPanel onError={setError} />}
      {tab === "topics" && <TopicsPanel onError={setError} />}
      {tab === "acls" && <AclsPanel onError={setError} />}
    </AppLayout>
  );
}

function LoginPage({ onLogin }: { onLogin: (token: string, username: string) => void }) {
  const [username, setUsername] = useState("admin");
  const [password, setPassword] = useState("admin123");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setLoading(true);
    setError("");
    try {
      const resp = await api.login(username, password);
      onLogin(resp.token, resp.username);
    } catch (err) {
      setError(err instanceof Error ? err.message : "登录失败");
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="login-page">
      <div className="auth-card">
        <div className="auth-card-head">
          <h5>Welcome Back !</h5>
          <p>Sign in to continue to HiMQTT.</p>
          <svg className="auth-illustration" viewBox="0 0 200 160" fill="none" aria-hidden="true">
            <rect x="120" y="30" width="60" height="80" rx="4" fill="#556ee6" opacity="0.15" />
            <rect x="130" y="40" width="40" height="28" rx="2" fill="#556ee6" opacity="0.35" />
            <circle cx="60" cy="90" r="28" fill="#556ee6" opacity="0.2" />
            <rect x="30" y="110" width="80" height="8" rx="4" fill="#556ee6" opacity="0.25" />
            <rect x="40" y="70" width="50" height="6" rx="3" fill="#34c38f" opacity="0.5" />
          </svg>
          <div className="auth-logo-badge">H</div>
        </div>
        <div className="auth-card-body">
          <form className="form-grid" onSubmit={submit}>
            <label>
              用户名
              <input value={username} onChange={(e) => setUsername(e.target.value)} placeholder="admin" />
            </label>
            <label>
              密码
              <input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="Enter Password"
              />
            </label>
            <button className="btn" type="submit" disabled={loading} style={{ width: "100%" }}>
              {loading ? "登录中..." : "Log In"}
            </button>
            {error && <div className="error-banner">{error}</div>}
          </form>
        </div>
      </div>
      <p className="auth-footer">© {new Date().getFullYear()} HiMQTT · MQTT Broker Admin</p>
    </div>
  );
}

function UsersPanel({ onError }: { onError: (msg: string) => void }) {
  const [users, setUsers] = useState<MqttUser[]>([]);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [enabled, setEnabled] = useState(true);
  const [editUser, setEditUser] = useState<MqttUser | null>(null);
  const [newPassword, setNewPassword] = useState("");

  async function load() {
    try {
      setUsers(await api.listMqttUsers());
      onError("");
    } catch (err) {
      onError(err instanceof Error ? err.message : "加载失败");
    }
  }

  useEffect(() => {
    load();
  }, []);

  return (
    <div className="card">
      <div className="card-body">
      <form
        className="inline-form"
        onSubmit={async (e) => {
          e.preventDefault();
          try {
            await api.createMqttUser({ username, password, enabled });
            setUsername("");
            setPassword("");
            await load();
          } catch (err) {
            onError(err instanceof Error ? err.message : "创建失败");
          }
        }}
      >
        <label>
          用户名
          <input value={username} onChange={(e) => setUsername(e.target.value)} required />
        </label>
        <label>
          密码
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            required
          />
        </label>
        <label className="checkbox-row">
          <input
            type="checkbox"
            checked={enabled}
            onChange={(e) => setEnabled(e.target.checked)}
          />
          启用
        </label>
        <button className="btn" type="submit">
          添加用户
        </button>
      </form>

      <table>
        <thead>
          <tr>
            <th>ID</th>
            <th>用户名</th>
            <th>状态</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          {users.map((user) => (
            <tr key={user.id}>
              <td>{user.id}</td>
              <td>{user.username}</td>
              <td>{user.enabled ? "启用" : "禁用"}</td>
              <td className="actions">
                <button
                  className="btn secondary sm"
                  onClick={() => {
                    setEditUser(user);
                    setNewPassword("");
                  }}
                >
                  修改密码
                </button>
                <button
                  className="btn secondary sm"
                  onClick={async () => {
                    try {
                      await api.updateMqttUser(user.id, { enabled: !user.enabled });
                      await load();
                    } catch (err) {
                      onError(err instanceof Error ? err.message : "更新失败");
                    }
                  }}
                >
                  {user.enabled ? "禁用" : "启用"}
                </button>
                <button
                  className="btn danger sm"
                  onClick={async () => {
                    if (!confirm(`删除用户 ${user.username}?`)) return;
                    try {
                      await api.deleteMqttUser(user.id);
                      await load();
                    } catch (err) {
                      onError(err instanceof Error ? err.message : "删除失败");
                    }
                  }}
                >
                  删除
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>

      <DetailModal
        title={editUser ? `修改密码 — ${editUser.username}` : ""}
        open={editUser !== null}
        onClose={() => setEditUser(null)}
      >
        <form
          className="form-grid"
          onSubmit={async (e) => {
            e.preventDefault();
            if (!editUser || newPassword.length < 4) {
              onError("密码至少 4 位");
              return;
            }
            try {
              await api.updateMqttUser(editUser.id, {
                password: newPassword,
                enabled: editUser.enabled,
              });
              setEditUser(null);
              setNewPassword("");
              onError("");
            } catch (err) {
              onError(err instanceof Error ? err.message : "修改密码失败");
            }
          }}
        >
          <label>
            新密码
            <input
              type="password"
              value={newPassword}
              onChange={(e) => setNewPassword(e.target.value)}
              required
              minLength={4}
            />
          </label>
          <button className="btn" type="submit">
            保存密码
          </button>
        </form>
      </DetailModal>
      </div>
    </div>
  );
}

function TopicsPanel({ onError }: { onError: (msg: string) => void }) {
  const [topics, setTopics] = useState<MqttTopic[]>([]);
  const [topic, setTopic] = useState("");
  const [description, setDescription] = useState("");

  async function load() {
    try {
      setTopics(await api.listTopics());
      onError("");
    } catch (err) {
      onError(err instanceof Error ? err.message : "加载失败");
    }
  }

  useEffect(() => {
    load();
  }, []);

  return (
    <div className="card">
      <div className="card-body">
      <p className="hint">启用 enforce_registered_topics 时，客户端只能访问此处登记的 Topic。</p>
      <form
        className="inline-form"
        onSubmit={async (e) => {
          e.preventDefault();
          try {
            await api.createTopic({ topic, description });
            setTopic("");
            setDescription("");
            await load();
          } catch (err) {
            onError(err instanceof Error ? err.message : "创建失败");
          }
        }}
      >
        <label>
          Topic
          <input value={topic} onChange={(e) => setTopic(e.target.value)} placeholder="home/#" required />
        </label>
        <label>
          描述
          <input value={description} onChange={(e) => setDescription(e.target.value)} />
        </label>
        <button className="btn" type="submit">
          添加 Topic
        </button>
      </form>

      <table>
        <thead>
          <tr>
            <th>ID</th>
            <th>Topic</th>
            <th>描述</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          {topics.map((item) => (
            <tr key={item.id}>
              <td>{item.id}</td>
              <td className="topic">{item.topic}</td>
              <td>{item.description}</td>
              <td>
                <button
                  className="btn danger sm"
                  onClick={async () => {
                    if (!confirm(`删除 Topic ${item.topic}?`)) return;
                    try {
                      await api.deleteTopic(item.id);
                      await load();
                    } catch (err) {
                      onError(err instanceof Error ? err.message : "删除失败");
                    }
                  }}
                >
                  删除
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      </div>
    </div>
  );
}

function AclsPanel({ onError }: { onError: (msg: string) => void }) {
  const [rules, setRules] = useState<AclRule[]>([]);
  const [form, setForm] = useState<CreateAclBody>({
    username: "",
    topic_pattern: "",
    can_subscribe: true,
    can_publish: false,
  });

  async function load() {
    try {
      setRules(await api.listAcls());
      onError("");
    } catch (err) {
      onError(err instanceof Error ? err.message : "加载失败");
    }
  }

  useEffect(() => {
    load();
  }, []);

  return (
    <div className="card">
      <div className="card-body">
      <form
        className="inline-form"
        onSubmit={async (e) => {
          e.preventDefault();
          try {
            await api.createAcl(form);
            setForm({
              username: "",
              topic_pattern: "",
              can_subscribe: true,
              can_publish: false,
            });
            await load();
          } catch (err) {
            onError(err instanceof Error ? err.message : "创建失败");
          }
        }}
      >
        <label>
          MQTT 用户名
          <input
            value={form.username}
            onChange={(e) => setForm({ ...form, username: e.target.value })}
            required
          />
        </label>
        <label>
          Topic 模式
          <input
            value={form.topic_pattern}
            onChange={(e) => setForm({ ...form, topic_pattern: e.target.value })}
            placeholder="home/+/temp"
            required
          />
        </label>
        <label className="checkbox-row">
          <input
            type="checkbox"
            checked={form.can_subscribe}
            onChange={(e) => setForm({ ...form, can_subscribe: e.target.checked })}
          />
          可订阅
        </label>
        <label className="checkbox-row">
          <input
            type="checkbox"
            checked={form.can_publish}
            onChange={(e) => setForm({ ...form, can_publish: e.target.checked })}
          />
          可发布
        </label>
        <button className="btn" type="submit">
          添加规则
        </button>
      </form>

      <table>
        <thead>
          <tr>
            <th>ID</th>
            <th>用户</th>
            <th>Topic 模式</th>
            <th>订阅</th>
            <th>发布</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          {rules.map((rule) => (
            <tr key={rule.id}>
              <td>{rule.id}</td>
              <td>{rule.username}</td>
              <td className="topic">{rule.topic_pattern}</td>
              <td>{rule.can_subscribe ? "是" : "否"}</td>
              <td>{rule.can_publish ? "是" : "否"}</td>
              <td className="actions">
                <button
                  className="btn secondary sm"
                  onClick={async () => {
                    try {
                      await api.updateAcl(rule.id, {
                        topic_pattern: rule.topic_pattern,
                        can_subscribe: !rule.can_subscribe,
                        can_publish: rule.can_publish,
                      });
                      await load();
                    } catch (err) {
                      onError(err instanceof Error ? err.message : "更新失败");
                    }
                  }}
                >
                  切换订阅
                </button>
                <button
                  className="btn secondary sm"
                  onClick={async () => {
                    try {
                      await api.updateAcl(rule.id, {
                        topic_pattern: rule.topic_pattern,
                        can_subscribe: rule.can_subscribe,
                        can_publish: !rule.can_publish,
                      });
                      await load();
                    } catch (err) {
                      onError(err instanceof Error ? err.message : "更新失败");
                    }
                  }}
                >
                  切换发布
                </button>
                <button
                  className="btn danger sm"
                  onClick={async () => {
                    if (!confirm(`删除 ACL #${rule.id}?`)) return;
                    try {
                      await api.deleteAcl(rule.id);
                      await load();
                    } catch (err) {
                      onError(err instanceof Error ? err.message : "删除失败");
                    }
                  }}
                >
                  删除
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      </div>
    </div>
  );
}
