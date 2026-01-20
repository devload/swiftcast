import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import Dashboard from './components/Dashboard';
import AccountManager from './components/AccountManager';
import SessionManager from './components/SessionManager';
import UsageMonitor from './components/UsageMonitor';
import Settings from './components/Settings';

function App() {
  const { t } = useTranslation();
  const [proxyRunning, setProxyRunning] = useState(false);
  const [activeAccount, setActiveAccount] = useState<any>(null);

  useEffect(() => {
    checkProxyStatus();
    loadActiveAccount();

    // 자동 업데이트: 2초마다 상태 확인
    const interval = setInterval(() => {
      checkProxyStatus();
      loadActiveAccount();
    }, 2000);

    return () => clearInterval(interval);
  }, []);

  const checkProxyStatus = async () => {
    try {
      const status = await invoke('get_proxy_status');
      setProxyRunning((status as any).running);
    } catch (error) {
      console.error('Failed to check proxy status:', error);
    }
  };

  const loadActiveAccount = async () => {
    try {
      const account = await invoke('get_active_account');
      setActiveAccount(account);
    } catch (error) {
      console.error('Failed to load active account:', error);
    }
  };

  return (
    <div className="min-h-screen bg-gray-50">
      {/* Header */}
      <header className="bg-white shadow-sm">
        <div className="max-w-7xl mx-auto px-4 py-4 sm:px-6 lg:px-8">
          <div className="flex items-center justify-between">
            <div>
              <h1 className="text-2xl font-bold text-gray-900">{t('app.title')}</h1>
              <p className="text-sm text-gray-500">{t('app.subtitle')}</p>
            </div>
            <div className="flex items-center space-x-4">
              <div className={`px-3 py-1 rounded-full text-sm font-medium ${
                proxyRunning
                  ? 'bg-green-100 text-green-800'
                  : 'bg-gray-100 text-gray-800'
              }`}>
                {proxyRunning ? `● ${t('app.status.running')}` : `○ ${t('app.status.stopped')}`}
              </div>
            </div>
          </div>
        </div>
      </header>

      {/* Main Content */}
      <main className="max-w-7xl mx-auto px-4 py-6 sm:px-6 lg:px-8">
        <div className="space-y-6">
          {/* Dashboard */}
          <Dashboard
            proxyRunning={proxyRunning}
            activeAccount={activeAccount}
            onProxyToggle={checkProxyStatus}
          />

          {/* Account Manager */}
          <AccountManager
            onAccountChange={loadActiveAccount}
          />

          {/* Session Manager */}
          <SessionManager />

          {/* Usage Monitor */}
          <UsageMonitor />

          {/* Settings */}
          <Settings />
        </div>
      </main>
    </div>
  );
}

export default App;
