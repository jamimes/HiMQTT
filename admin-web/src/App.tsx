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
  UserCategory,
  UserDefaultsSettings,
} from "./api";
import { AppLayout, NavKey } from "./components/AppLayout";
import { DetailModal } from "./components/DetailModal";
import { MonitorPage } from "./pages/MonitorPage";

function formatLoginTime(iso: string | null) {
  if (!iso) return "—";
  try {
    return new Date(iso).toLocaleString();
  } catch {
    return iso;
  }
}

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
      {tab === "categories" && <CategoriesPanel onError={setError} />}
      {tab === "topics" && <TopicsPanel onError={setError} />}
      {tab === "acls" && <AclsPanel onError={setError} />}
      {tab === "settings" && <SettingsPanel onError={setError} />}
    </AppLayout>
  );
}

function LoginPage({ onLogin }: { onLogin: (token: string, username: string) => void }) {
  const [username, setUsername] = useState("admin");
  const [password, setPassword] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

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
      <div className="login-card card">
        <div className="card-body">
          <h1>HiMQTT</h1>
          <p className="login-sub">管理控制台</p>
          <form className="form-grid" onSubmit={submit}>
            <label>
              用户名
              <input
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                placeholder="Enter username"
              />
            </label>
            <label>
              密码
              <input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="Enter password"
              />
            </label>
            <button className="btn" type="submit" disabled={loading} style={{ width: "100%" }}>
              {loading ? "登录中..." : "Sign In"}
            </button>
            {error && <div className="error-banner">{error}</div>}
          </form>
        </div>
      </div>
    </div>
  );
}

function UsersPanel({ onError }: { onError: (msg: string) => void }) {
  const [users, setUsers] = useState<MqttUser[]>([]);
  const [categories, setCategories] = useState<UserCategory[]>([]);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [enabled, setEnabled] = useState(true);
  const [categoryId, setCategoryId] = useState<number | "">("");
  const [applyDefaults, setApplyDefaults] = useState(true);
  const [editUser, setEditUser] = useState<MqttUser | null>(null);
  const [newPassword, setNewPassword] = useState("");
  const [batchOpen, setBatchOpen] = useState(false);
  const [batchText, setBatchText] = useState("");
  const [batchCategoryId, setBatchCategoryId] = useState<number | "">("");
  const [batchApplyDefaults, setBatchApplyDefaults] = useState(true);
  const [manageUser, setManageUser] = useState<MqttUser | null>(null);
  const [userTopics, setUserTopics] = useState<MqttTopic[]>([]);
  const [userAcls, setUserAcls] = useState<AclRule[]>([]);
  const [topicForm, setTopicForm] = useState({ topic: "", description: "" });
  const [aclForm, setAclForm] = useState({
    topic_pattern: "",
    can_subscribe: true,
    can_publish: true,
  });

  async function load() {
    try {
      const [u, c] = await Promise.all([api.listMqttUsers(), api.listCategories()]);
      setUsers(u);
      setCategories(c);
      onError("");
    } catch (err) {
      onError(err instanceof Error ? err.message : "加载失败");
    }
  }

  async function openManage(user: MqttUser) {
    try {
      const res = await api.getUserResources(user.id);
      setManageUser(res.user);
      setUserTopics(res.topics);
      setUserAcls(res.acls);
      setTopicForm({ topic: "", description: "" });
      setAclForm({ topic_pattern: "", can_subscribe: true, can_publish: true });
      onError("");
    } catch (err) {
      onError(err instanceof Error ? err.message : "加载用户资源失败");
    }
  }

  async function reloadManage() {
    if (!manageUser) return;
    await openManage(manageUser);
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
              await api.createMqttUser({
                username,
                password,
                enabled,
                category_id: categoryId === "" ? null : categoryId,
                apply_defaults: applyDefaults,
              });
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
          <label>
            分类
            <select
              value={categoryId}
              onChange={(e) =>
                setCategoryId(e.target.value === "" ? "" : Number(e.target.value))
              }
            >
              <option value="">无</option>
              {categories.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </select>
          </label>
          <label className="checkbox-row">
            <input
              type="checkbox"
              checked={enabled}
              onChange={(e) => setEnabled(e.target.checked)}
            />
            启用
          </label>
          <label className="checkbox-row">
            <input
              type="checkbox"
              checked={applyDefaults}
              onChange={(e) => setApplyDefaults(e.target.checked)}
            />
            应用默认 Topic/ACL
          </label>
          <button className="btn" type="submit">
            添加用户
          </button>
          <button className="btn secondary" type="button" onClick={() => setBatchOpen(true)}>
            批量导入
          </button>
        </form>

        <table>
          <thead>
            <tr>
              <th>ID</th>
              <th>用户名</th>
              <th>分类</th>
              <th>状态</th>
              <th>最后登录</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {users.map((user) => (
              <tr key={user.id}>
                <td>{user.id}</td>
                <td>{user.username}</td>
                <td>{user.category_name || "—"}</td>
                <td>{user.enabled ? "启用" : "禁用"}</td>
                <td>
                  <div>{formatLoginTime(user.last_login_at)}</div>
                  <div className="hint">{user.last_login_ip || ""}</div>
                </td>
                <td className="actions">
                  <button className="btn secondary sm" onClick={() => openManage(user)}>
                    Topic/ACL
                  </button>
                  <button
                    className="btn secondary sm"
                    onClick={() => {
                      setEditUser(user);
                      setNewPassword("");
                    }}
                  >
                    编辑
                  </button>
                  <button
                    className="btn secondary sm"
                    onClick={async () => {
                      try {
                        await api.updateMqttUser(user.id, {
                          enabled: !user.enabled,
                          category_id: user.category_id,
                        });
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
          title={editUser ? `编辑用户 — ${editUser.username}` : ""}
          open={editUser !== null}
          onClose={() => setEditUser(null)}
        >
          <form
            className="form-grid"
            onSubmit={async (e) => {
              e.preventDefault();
              if (!editUser) return;
              if (newPassword && newPassword.length < 4) {
                onError("密码至少 4 位");
                return;
              }
              try {
                await api.updateMqttUser(editUser.id, {
                  password: newPassword || undefined,
                  enabled: editUser.enabled,
                  category_id: editUser.category_id,
                });
                setEditUser(null);
                setNewPassword("");
                onError("");
                await load();
              } catch (err) {
                onError(err instanceof Error ? err.message : "保存失败");
              }
            }}
          >
            <label>
              新密码（留空则不修改）
              <input
                type="password"
                value={newPassword}
                onChange={(e) => setNewPassword(e.target.value)}
                minLength={4}
              />
            </label>
            <label>
              分类
              <select
                value={editUser?.category_id ?? ""}
                onChange={(e) => {
                  if (!editUser) return;
                  setEditUser({
                    ...editUser,
                    category_id: e.target.value === "" ? null : Number(e.target.value),
                  });
                }}
              >
                <option value="">无</option>
                {categories.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </select>
            </label>
            <button className="btn" type="submit">
              保存
            </button>
          </form>
        </DetailModal>

        <DetailModal
          title="批量导入 MQTT 用户"
          open={batchOpen}
          onClose={() => setBatchOpen(false)}
        >
          <form
            className="form-grid"
            onSubmit={async (e) => {
              e.preventDefault();
              try {
                const result = await api.batchCreateMqttUsers({
                  text: batchText,
                  enabled: true,
                  category_id: batchCategoryId === "" ? null : batchCategoryId,
                  apply_defaults: batchApplyDefaults,
                });
                setBatchOpen(false);
                setBatchText("");
                await load();
                const msg = [
                  `成功创建 ${result.created} 个`,
                  result.skipped.length ? `跳过: ${result.skipped.join(", ")}` : "",
                  result.errors.length ? `错误: ${result.errors.join("; ")}` : "",
                ]
                  .filter(Boolean)
                  .join("。");
                onError(msg);
              } catch (err) {
                onError(err instanceof Error ? err.message : "批量导入失败");
              }
            }}
          >
            <p className="hint">每行格式：用户名 密码（空格或 Tab 分隔）。以 # 开头的行为注释。</p>
            <label>
              内容
              <textarea
                rows={10}
                value={batchText}
                onChange={(e) => setBatchText(e.target.value)}
                placeholder={"device001 pass123\ndevice002 pass456"}
                required
              />
            </label>
            <label>
              分类
              <select
                value={batchCategoryId}
                onChange={(e) =>
                  setBatchCategoryId(e.target.value === "" ? "" : Number(e.target.value))
                }
              >
                <option value="">无</option>
                {categories.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </select>
            </label>
            <label className="checkbox-row">
              <input
                type="checkbox"
                checked={batchApplyDefaults}
                onChange={(e) => setBatchApplyDefaults(e.target.checked)}
              />
              应用默认 Topic/ACL
            </label>
            <button className="btn" type="submit">
              开始导入
            </button>
          </form>
        </DetailModal>

        <DetailModal
          title={manageUser ? `用户资源 — ${manageUser.username}` : ""}
          open={manageUser !== null}
          onClose={() => setManageUser(null)}
        >
          {manageUser && (
            <div className="form-grid">
              <h3>Topic 目录</h3>
              <form
                className="inline-form"
                onSubmit={async (e) => {
                  e.preventDefault();
                  try {
                    await api.createTopic({
                      topic: topicForm.topic,
                      description: topicForm.description,
                      owner_username: manageUser.username,
                    });
                    setTopicForm({ topic: "", description: "" });
                    await reloadManage();
                  } catch (err) {
                    onError(err instanceof Error ? err.message : "添加 Topic 失败");
                  }
                }}
              >
                <label>
                  Topic
                  <input
                    value={topicForm.topic}
                    onChange={(e) => setTopicForm({ ...topicForm, topic: e.target.value })}
                    required
                  />
                </label>
                <label>
                  描述
                  <input
                    value={topicForm.description}
                    onChange={(e) =>
                      setTopicForm({ ...topicForm, description: e.target.value })
                    }
                  />
                </label>
                <button className="btn sm" type="submit">
                  添加
                </button>
              </form>
              <table>
                <thead>
                  <tr>
                    <th>Topic</th>
                    <th>描述</th>
                    <th>操作</th>
                  </tr>
                </thead>
                <tbody>
                  {userTopics.map((t) => (
                    <tr key={t.id}>
                      <td className="topic">{t.topic}</td>
                      <td>{t.description}</td>
                      <td>
                        <button
                          className="btn danger sm"
                          onClick={async () => {
                            if (!confirm(`删除 Topic ${t.topic}?`)) return;
                            try {
                              await api.deleteTopic(t.id);
                              await reloadManage();
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
                  {userTopics.length === 0 && (
                    <tr>
                      <td colSpan={3}>暂无归属该用户的 Topic</td>
                    </tr>
                  )}
                </tbody>
              </table>

              <h3>ACL 规则</h3>
              <form
                className="inline-form"
                onSubmit={async (e) => {
                  e.preventDefault();
                  try {
                    await api.createAcl({
                      username: manageUser.username,
                      ...aclForm,
                    });
                    setAclForm({
                      topic_pattern: "",
                      can_subscribe: true,
                      can_publish: true,
                    });
                    await reloadManage();
                  } catch (err) {
                    onError(err instanceof Error ? err.message : "添加 ACL 失败");
                  }
                }}
              >
                <label>
                  Topic 模式
                  <input
                    value={aclForm.topic_pattern}
                    onChange={(e) =>
                      setAclForm({ ...aclForm, topic_pattern: e.target.value })
                    }
                    required
                  />
                </label>
                <label className="checkbox-row">
                  <input
                    type="checkbox"
                    checked={aclForm.can_subscribe}
                    onChange={(e) =>
                      setAclForm({ ...aclForm, can_subscribe: e.target.checked })
                    }
                  />
                  可订阅
                </label>
                <label className="checkbox-row">
                  <input
                    type="checkbox"
                    checked={aclForm.can_publish}
                    onChange={(e) =>
                      setAclForm({ ...aclForm, can_publish: e.target.checked })
                    }
                  />
                  可发布
                </label>
                <button className="btn sm" type="submit">
                  添加
                </button>
              </form>
              <table>
                <thead>
                  <tr>
                    <th>模式</th>
                    <th>订阅</th>
                    <th>发布</th>
                    <th>操作</th>
                  </tr>
                </thead>
                <tbody>
                  {userAcls.map((rule) => (
                    <tr key={rule.id}>
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
                              await reloadManage();
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
                              await reloadManage();
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
                              await reloadManage();
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
                  {userAcls.length === 0 && (
                    <tr>
                      <td colSpan={4}>暂无 ACL 规则</td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>
          )}
        </DetailModal>
      </div>
    </div>
  );
}

function CategoriesPanel({ onError }: { onError: (msg: string) => void }) {
  const [items, setItems] = useState<UserCategory[]>([]);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [edit, setEdit] = useState<UserCategory | null>(null);

  async function load() {
    try {
      setItems(await api.listCategories());
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
              await api.createCategory({ name, description });
              setName("");
              setDescription("");
              await load();
            } catch (err) {
              onError(err instanceof Error ? err.message : "创建失败");
            }
          }}
        >
          <label>
            名称
            <input value={name} onChange={(e) => setName(e.target.value)} required />
          </label>
          <label>
            描述
            <input value={description} onChange={(e) => setDescription(e.target.value)} />
          </label>
          <button className="btn" type="submit">
            添加分类
          </button>
        </form>

        <table>
          <thead>
            <tr>
              <th>ID</th>
              <th>名称</th>
              <th>描述</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {items.map((item) => (
              <tr key={item.id}>
                <td>{item.id}</td>
                <td>{item.name}</td>
                <td>{item.description}</td>
                <td className="actions">
                  <button className="btn secondary sm" onClick={() => setEdit(item)}>
                    编辑
                  </button>
                  <button
                    className="btn danger sm"
                    onClick={async () => {
                      if (!confirm(`删除分类 ${item.name}?`)) return;
                      try {
                        await api.deleteCategory(item.id);
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
          title={edit ? `编辑分类 — ${edit.name}` : ""}
          open={edit !== null}
          onClose={() => setEdit(null)}
        >
          {edit && (
            <form
              className="form-grid"
              onSubmit={async (e) => {
                e.preventDefault();
                try {
                  await api.updateCategory(edit.id, {
                    name: edit.name,
                    description: edit.description,
                  });
                  setEdit(null);
                  await load();
                } catch (err) {
                  onError(err instanceof Error ? err.message : "更新失败");
                }
              }}
            >
              <label>
                名称
                <input
                  value={edit.name}
                  onChange={(e) => setEdit({ ...edit, name: e.target.value })}
                  required
                />
              </label>
              <label>
                描述
                <input
                  value={edit.description}
                  onChange={(e) => setEdit({ ...edit, description: e.target.value })}
                />
              </label>
              <button className="btn" type="submit">
                保存
              </button>
            </form>
          )}
        </DetailModal>
      </div>
    </div>
  );
}

function SettingsPanel({ onError }: { onError: (msg: string) => void }) {
  const [settings, setSettings] = useState<UserDefaultsSettings>({
    default_topics: [],
    default_acls: [],
  });
  const [saving, setSaving] = useState(false);

  async function load() {
    try {
      setSettings(await api.getUserDefaults());
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
        <p className="hint">
          新建 MQTT 用户时可勾选「应用默认 Topic/ACL」。模板中可用 {"{username}"} 占位符。
        </p>

        <h3>默认 Topic 目录</h3>
        {settings.default_topics.map((item, idx) => (
          <div className="inline-form" key={`topic-${idx}`}>
            <label>
              Topic
              <input
                value={item.topic}
                onChange={(e) => {
                  const next = [...settings.default_topics];
                  next[idx] = { ...item, topic: e.target.value };
                  setSettings({ ...settings, default_topics: next });
                }}
              />
            </label>
            <label>
              描述
              <input
                value={item.description}
                onChange={(e) => {
                  const next = [...settings.default_topics];
                  next[idx] = { ...item, description: e.target.value };
                  setSettings({ ...settings, default_topics: next });
                }}
              />
            </label>
            <button
              className="btn danger sm"
              type="button"
              onClick={() => {
                setSettings({
                  ...settings,
                  default_topics: settings.default_topics.filter((_, i) => i !== idx),
                });
              }}
            >
              删除
            </button>
          </div>
        ))}
        <button
          className="btn secondary sm"
          type="button"
          onClick={() =>
            setSettings({
              ...settings,
              default_topics: [
                ...settings.default_topics,
                { topic: "devices/{username}/data", description: "" },
              ],
            })
          }
        >
          添加 Topic 模板
        </button>

        <h3>默认 ACL 规则</h3>
        {settings.default_acls.map((item, idx) => (
          <div className="inline-form" key={`acl-${idx}`}>
            <label>
              Topic 模式
              <input
                value={item.topic_pattern}
                onChange={(e) => {
                  const next = [...settings.default_acls];
                  next[idx] = { ...item, topic_pattern: e.target.value };
                  setSettings({ ...settings, default_acls: next });
                }}
              />
            </label>
            <label className="checkbox-row">
              <input
                type="checkbox"
                checked={item.can_subscribe}
                onChange={(e) => {
                  const next = [...settings.default_acls];
                  next[idx] = { ...item, can_subscribe: e.target.checked };
                  setSettings({ ...settings, default_acls: next });
                }}
              />
              可订阅
            </label>
            <label className="checkbox-row">
              <input
                type="checkbox"
                checked={item.can_publish}
                onChange={(e) => {
                  const next = [...settings.default_acls];
                  next[idx] = { ...item, can_publish: e.target.checked };
                  setSettings({ ...settings, default_acls: next });
                }}
              />
              可发布
            </label>
            <button
              className="btn danger sm"
              type="button"
              onClick={() => {
                setSettings({
                  ...settings,
                  default_acls: settings.default_acls.filter((_, i) => i !== idx),
                });
              }}
            >
              删除
            </button>
          </div>
        ))}
        <button
          className="btn secondary sm"
          type="button"
          onClick={() =>
            setSettings({
              ...settings,
              default_acls: [
                ...settings.default_acls,
                {
                  topic_pattern: "devices/{username}/#",
                  can_subscribe: true,
                  can_publish: true,
                },
              ],
            })
          }
        >
          添加 ACL 模板
        </button>

        <div style={{ marginTop: 16 }}>
          <button
            className="btn"
            type="button"
            disabled={saving}
            onClick={async () => {
              setSaving(true);
              try {
                setSettings(await api.updateUserDefaults(settings));
                onError("");
              } catch (err) {
                onError(err instanceof Error ? err.message : "保存失败");
              } finally {
                setSaving(false);
              }
            }}
          >
            {saving ? "保存中..." : "保存设置"}
          </button>
        </div>
      </div>
    </div>
  );
}

function TopicsPanel({ onError }: { onError: (msg: string) => void }) {
  const [topics, setTopics] = useState<MqttTopic[]>([]);
  const [topic, setTopic] = useState("");
  const [description, setDescription] = useState("");
  const [owner, setOwner] = useState("");

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
              await api.createTopic({
                topic,
                description,
                owner_username: owner.trim() || null,
              });
              setTopic("");
              setDescription("");
              setOwner("");
              await load();
            } catch (err) {
              onError(err instanceof Error ? err.message : "创建失败");
            }
          }}
        >
          <label>
            Topic
            <input
              value={topic}
              onChange={(e) => setTopic(e.target.value)}
              placeholder="home/#"
              required
            />
          </label>
          <label>
            描述
            <input value={description} onChange={(e) => setDescription(e.target.value)} />
          </label>
          <label>
            归属用户
            <input
              value={owner}
              onChange={(e) => setOwner(e.target.value)}
              placeholder="可选"
            />
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
              <th>归属用户</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {topics.map((item) => (
              <tr key={item.id}>
                <td>{item.id}</td>
                <td className="topic">{item.topic}</td>
                <td>{item.description}</td>
                <td>{item.owner_username || "—"}</td>
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
