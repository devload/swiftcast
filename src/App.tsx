import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import Dashboard from './components/Dashboard';
import AccountManager from './components/AccountManager';
import UsageMonitor from './components/UsageMonitor';

function App() {
  const [proxyRunning, setProxyRunning] = useState(false);
  const [activeAccount, setActiveAccount] = useState<any>(null);

  useEffect(() => {
    checkProxyStatus();
    loadActiveAccount();
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
              <h1 className="text-2xl font-bold text-gray-900">SwiftCast</h1>
              <p className="text-sm text-gray-500">AI Provider 스위칭 & 사용량 모니터링</p>
            </div>
            <div className="flex items-center space-x-4">
              <div className={`px-3 py-1 rounded-full text-sm font-medium ${
                proxyRunning
                  ? 'bg-green-100 text-green-800'
                  : 'bg-gray-100 text-gray-800'
              }`}>
                {proxyRunning ? '● 실행 중' : '○ 중지됨'}
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

          {/* Usage Monitor */}
          <UsageMonitor />
        </div>
      </main>
    </div>
  );
}

export default App;
