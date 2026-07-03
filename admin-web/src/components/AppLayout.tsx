import { ReactNode, useEffect, useState } from "react";
import {
  IconDashboard,
  IconMenu,
  IconSearch,
  IconShield,
  IconTopic,
  IconUsers,
} from "./Icons";

export type NavKey = "monitor" | "users" | "topics" | "acls";

const NAV: { key: NavKey; label: string; icon: typeof IconDashboard }[] = [
  { key: "monitor", label: "连接监控", icon: IconDashboard },
  { key: "users", label: "MQTT 用户", icon: IconUsers },
  { key: "topics", label: "Topic 目录", icon: IconTopic },
  { key: "acls", label: "ACL 规则", icon: IconShield },
];

const TITLES: Record<NavKey, { title: string; breadcrumb: string }> = {
  monitor: { title: "连接监控", breadcrumb: "Dashboards" },
  users: { title: "MQTT 用户", breadcrumb: "管理" },
  topics: { title: "Topic 目录", breadcrumb: "管理" },
  acls: { title: "ACL 规则", breadcrumb: "管理" },
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
  const meta = TITLES[active];
  const initials = username.slice(0, 1).toUpperCase();

  useEffect(() => {
    document.body.classList.add("layout-fixed");
    return () => document.body.classList.remove("layout-fixed");
  }, []);

  return (
    <div
      id="layout-wrapper"
      className={sidebarExpanded ? "" : "sidebar-collapsed"}
    >
      <header id="page-topbar">
        <div className="navbar-header">
          <button
            type="button"
            className="btn-icon btn-topbar"
            onClick={() => setSidebarExpanded((v) => !v)}
            aria-label={sidebarExpanded ? "折叠侧边栏" : "展开侧边栏"}
            aria-expanded={sidebarExpanded}
          >
            <IconMenu />
          </button>

          <form className="app-search" onSubmit={(e) => e.preventDefault()}>
            <span className="search-icon">
              <IconSearch />
            </span>
            <input type="search" placeholder="搜索..." />
          </form>

          <div className="topbar-right">
            <div className="topbar-user">
              <span className="user-name">{username}</span>
              <span className="user-avatar">{initials}</span>
            </div>
            <button type="button" className="btn btn-sm btn-light" onClick={onLogout}>
              退出
            </button>
          </div>
        </div>
      </header>

      <aside className="vertical-menu">
        <div className="navbar-brand-box">
          <span className="logo-lg">HiMQTT</span>
          <span className="logo-sm">H</span>
        </div>

        <div className="sidebar-menu-scroll">
          <p className="menu-title">Menu</p>
          <ul className="metismenu">
            {NAV.map((item) => {
              const Icon = item.icon;
              return (
                <li key={item.key} className={active === item.key ? "mm-active" : ""}>
                  <button
                    type="button"
                    className={`waves-effect${active === item.key ? " active" : ""}`}
                    title={item.label}
                    onClick={() => onNavigate(item.key)}
                  >
                    <Icon />
                    <span>{item.label}</span>
                  </button>
                </li>
              );
            })}
          </ul>
        </div>
      </aside>

      <div className="main-content">
        <div className="page-content">
          <div className="container-fluid">
            <div className="page-title-box">
              <div className="page-title-left">
                <ol className="breadcrumb">
                  <li className="breadcrumb-item">HiMQTT</li>
                  <li className="breadcrumb-item">{meta.breadcrumb}</li>
                  <li className="breadcrumb-item active">{meta.title}</li>
                </ol>
                <h4 className="page-title">{meta.title}</h4>
              </div>
            </div>
            {children}
          </div>
        </div>
        <footer className="footer">
          <div className="container-fluid">
            <div className="row">
              <div className="col-sm-6">© {new Date().getFullYear()} HiMQTT</div>
              <div className="col-sm-6">
                <div className="text-sm-end">MQTT Broker 管理控制台</div>
              </div>
            </div>
          </div>
        </footer>
      </div>
    </div>
  );
}
