import { ReactNode, useEffect, useState } from "react";
import {
  IconDashboard,
  IconFolder,
  IconMenu,
  IconSearch,
  IconSettings,
  IconShield,
  IconTopic,
  IconUsers,
} from "./Icons";

export type NavKey = "monitor" | "users" | "categories" | "topics" | "acls" | "settings";

const NAV: { key: NavKey; label: string; icon: typeof IconDashboard }[] = [
  { key: "monitor", label: "连接监控", icon: IconDashboard },
  { key: "users", label: "MQTT 用户", icon: IconUsers },
  { key: "categories", label: "用户分类", icon: IconFolder },
  { key: "topics", label: "Topic 目录", icon: IconTopic },
  { key: "acls", label: "ACL 规则", icon: IconShield },
  { key: "settings", label: "系统设置", icon: IconSettings },
];

const PAGE_META: Record<
  NavKey,
  { title: string; subtitle: string; breadcrumb: string }
> = {
  monitor: {
    title: "Hi, welcome back!",
    subtitle: "实时查看 MQTT 连接、消息流与 Topic 统计。",
    breadcrumb: "Dashboard",
  },
  users: {
    title: "MQTT 用户",
    subtitle: "管理客户端账号、Topic、ACL 与批量导入。",
    breadcrumb: "用户管理",
  },
  categories: {
    title: "用户分类",
    subtitle: "为 MQTT 用户划分业务类别。",
    breadcrumb: "用户分类",
  },
  topics: {
    title: "Topic 目录",
    subtitle: "预注册可访问的 Topic 白名单。",
    breadcrumb: "Topic",
  },
  acls: {
    title: "ACL 规则",
    subtitle: "配置每个用户的订阅与发布权限。",
    breadcrumb: "ACL",
  },
  settings: {
    title: "系统设置",
    subtitle: "配置新建用户时的默认 Topic 与 ACL 模板。",
    breadcrumb: "系统设置",
  },
};

type Props = {
  active: NavKey;
  username: string;
  onNavigate: (key: NavKey) => void;
  onLogout: () => void;
  children: ReactNode;
};

export function AppLayout({
  active,
  username,
  onNavigate,
  onLogout,
  children,
}: Props) {
  const [sidebarExpanded, setSidebarExpanded] = useState(true);
  const meta = PAGE_META[active];
  const initials = username.slice(0, 1).toUpperCase();

  useEffect(() => {
    document.body.classList.add("layout-fixed");
    return () => document.body.classList.remove("layout-fixed");
  }, []);

  return (
    <div className={`page${sidebarExpanded ? "" : " sidebar-collapsed"}`}>
      <aside className="app-sidebar">
        <div className="main-sidebar-header">
          <span className="brand-logo">HiMQTT</span>
          <span className="brand-icon">H</span>
        </div>
        <nav className="main-sidebar">
          <ul className="side-menu">
            <li className="slide__category">
              <span className="category-name">Main</span>
            </li>
            {NAV.map((item) => {
              const Icon = item.icon;
              const isActive = active === item.key;
              return (
                <li key={item.key} className={`slide${isActive ? " is-expanded" : ""}`}>
                  <button
                    type="button"
                    className={`side-menu__item${isActive ? " active" : ""}`}
                    title={item.label}
                    onClick={() => onNavigate(item.key)}
                  >
                    <span className="side-menu__icon">
                      <Icon size={20} />
                    </span>
                    <span className="side-menu__label">{item.label}</span>
                  </button>
                </li>
              );
            })}
          </ul>
        </nav>
      </aside>

      <header className="app-header">
        <div className="header-content-left">
          <button
            type="button"
            className="header-link sidemenu-toggle"
            onClick={() => setSidebarExpanded((v) => !v)}
            aria-label={sidebarExpanded ? "折叠侧边栏" : "展开侧边栏"}
          >
            <IconMenu />
          </button>
          <form className="header-search" onSubmit={(e) => e.preventDefault()}>
            <IconSearch />
            <input type="search" placeholder="搜索..." />
          </form>
        </div>
        <div className="header-content-right">
          <div className="header-profile">
            <span className="profile-name">{username}</span>
            <span className="profile-avatar">{initials}</span>
          </div>
          <button type="button" className="btn btn-outline-primary btn-sm" onClick={onLogout}>
            退出
          </button>
        </div>
      </header>

      <main className="main-content app-content">
        <div className="main-container container-fluid">
          <div className="page-header-block">
            <div>
              <nav className="breadcrumb-nav" aria-label="breadcrumb">
                <span>HiMQTT</span>
                <span className="sep">/</span>
                <span>{meta.breadcrumb}</span>
              </nav>
              <h1 className="page-heading">{meta.title}</h1>
              <p className="page-subheading">{meta.subtitle}</p>
            </div>
          </div>
          {children}
        </div>
        <footer className="footer">
          <div className="container-fluid footer-inner">
            <span>© {new Date().getFullYear()} HiMQTT</span>
            <span>MQTT Broker 管理控制台</span>
          </div>
        </footer>
      </main>
    </div>
  );
}
