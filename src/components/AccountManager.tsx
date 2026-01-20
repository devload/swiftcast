import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';

interface Account {
  id: string;
  name: string;
  base_url: string;
  created_at: number;
  is_active: boolean;
}

interface AccountManagerProps {
  onAccountChange: () => void;
}

export default function AccountManager({ onAccountChange }: AccountManagerProps) {
  const { t } = useTranslation();
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [showAddForm, setShowAddForm] = useState(false);
  const [newAccount, setNewAccount] = useState({
    name: '',
    base_url: '',
    api_key: ''
  });
  const [scanMessages, setScanMessages] = useState<string[]>([]);
  const [scanning, setScanning] = useState(false);

  useEffect(() => {
    loadAccounts();

    const interval = setInterval(() => {
      loadAccounts();
    }, 2000);

    return () => clearInterval(interval);
  }, []);

  const loadAccounts = async () => {
    try {
      const result = await invoke('get_accounts');
      setAccounts(result as Account[]);
    } catch (error) {
      console.error('Failed to load accounts:', error);
    }
  };

  const handleAddAccount = async () => {
    if (!newAccount.name || !newAccount.base_url || !newAccount.api_key) {
      alert(t('accounts.fillAllFields'));
      return;
    }

    try {
      await invoke('create_account', {
        name: newAccount.name,
        baseUrl: newAccount.base_url,
        apiKey: newAccount.api_key
      });
      setNewAccount({ name: '', base_url: '', api_key: '' });
      setShowAddForm(false);
      await loadAccounts();
      onAccountChange();
    } catch (error) {
      console.error('Failed to create account:', error);
    }
  };

  const handleSwitchAccount = async (accountId: string) => {
    try {
      await invoke('switch_account', { accountId });
      await loadAccounts();
      onAccountChange();
    } catch (error) {
      console.error('Failed to switch account:', error);
    }
  };

  const handleDeleteAccount = async (accountId: string) => {
    if (!confirm(t('accounts.confirmDelete'))) {
      return;
    }

    try {
      await invoke('delete_account', { accountId });
      await loadAccounts();
      onAccountChange();
    } catch (error) {
      console.error('Failed to delete account:', error);
    }
  };

  const handleAutoScan = async () => {
    setScanning(true);
    setScanMessages([]);

    try {
      const result = await invoke<{
        found_accounts: number;
        imported_accounts: number;
        messages: string[];
      }>('auto_scan_accounts');

      setScanMessages(result.messages);
      await loadAccounts();
      onAccountChange();

      setTimeout(() => setScanMessages([]), 5000);
    } catch (error) {
      console.error('Failed to auto scan:', error);
      setScanMessages([`❌ Scan failed: ${error}`]);
    } finally {
      setScanning(false);
    }
  };

  return (
    <div className="bg-white rounded-lg shadow p-6">
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-lg font-semibold text-gray-900">{t('accounts.title')}</h2>
        <div className="flex space-x-2">
          <button
            onClick={handleAutoScan}
            disabled={scanning}
            className="bg-purple-600 hover:bg-purple-700 disabled:bg-gray-300 disabled:cursor-not-allowed text-white text-sm font-medium py-2 px-4 rounded-lg transition-colors"
          >
            {scanning ? `🔍 ${t('accounts.scanning')}` : `🔍 ${t('accounts.autoScan')}`}
          </button>
          <button
            onClick={() => setShowAddForm(!showAddForm)}
            className="bg-blue-600 hover:bg-blue-700 text-white text-sm font-medium py-2 px-4 rounded-lg transition-colors"
          >
            {showAddForm ? t('accounts.cancel') : `+ ${t('accounts.addAccount')}`}
          </button>
        </div>
      </div>

      {/* 스캔 메시지 */}
      {scanMessages.length > 0 && (
        <div className="mb-4 bg-gray-900 text-green-400 rounded-lg p-4 space-y-1">
          {scanMessages.map((msg, idx) => (
            <div key={idx} className="text-sm font-mono">{msg}</div>
          ))}
        </div>
      )}

      {/* 계정 추가 폼 */}
      {showAddForm && (
        <div className="bg-gray-50 rounded-lg p-4 mb-4">
          <div className="space-y-3">
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">
                {t('accounts.accountName')}
              </label>
              <input
                type="text"
                value={newAccount.name}
                onChange={(e) => setNewAccount({ ...newAccount, name: e.target.value })}
                placeholder={t('accounts.accountNamePlaceholder')}
                className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">
                {t('accounts.baseUrl')}
              </label>
              <select
                value={newAccount.base_url}
                onChange={(e) => setNewAccount({ ...newAccount, base_url: e.target.value })}
                className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 bg-white appearance-none"
                style={{
                  backgroundImage: `url("data:image/svg+xml,%3csvg xmlns='http://www.w3.org/2000/svg' fill='none' viewBox='0 0 20 20'%3e%3cpath stroke='%236b7280' stroke-linecap='round' stroke-linejoin='round' stroke-width='1.5' d='M6 8l4 4 4-4'/%3e%3c/svg%3e")`,
                  backgroundPosition: 'right 0.5rem center',
                  backgroundRepeat: 'no-repeat',
                  backgroundSize: '1.5em 1.5em',
                  paddingRight: '2.5rem'
                }}
              >
                <option value="">{t('accounts.selectBaseUrl')}</option>
                <option value="https://api.anthropic.com">{t('accounts.anthropic')}</option>
                <option value="https://api.z.ai/api/anthropic">{t('accounts.glm')}</option>
              </select>
            </div>
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">
                {t('accounts.apiKey')}
              </label>
              <input
                type="password"
                value={newAccount.api_key}
                onChange={(e) => setNewAccount({ ...newAccount, api_key: e.target.value })}
                placeholder={t('accounts.apiKeyPlaceholder')}
                className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
              />
            </div>
            <button
              onClick={handleAddAccount}
              className="w-full bg-blue-600 hover:bg-blue-700 text-white font-medium py-2 px-4 rounded-lg transition-colors"
            >
              {t('accounts.add')}
            </button>
          </div>
        </div>
      )}

      {/* 계정 목록 */}
      <div className="space-y-2">
        {accounts.length === 0 ? (
          <div className="text-center text-gray-500 py-8">
            {t('accounts.noAccounts')}
          </div>
        ) : (
          accounts.map((account) => (
            <div
              key={account.id}
              className={`border rounded-lg p-4 ${
                account.is_active
                  ? 'border-blue-500 bg-blue-50'
                  : 'border-gray-200 bg-white hover:bg-gray-50'
              }`}
            >
              <div className="flex items-center justify-between">
                <div className="flex-1">
                  <div className="flex items-center space-x-2">
                    <h3 className="font-medium text-gray-900">{account.name}</h3>
                    {account.is_active && (
                      <span className="bg-blue-600 text-white text-xs px-2 py-0.5 rounded-full">
                        {t('accounts.active')}
                      </span>
                    )}
                  </div>
                  <p className="text-sm text-gray-600 mt-1">{account.base_url}</p>
                </div>
                <div className="flex items-center space-x-2">
                  {!account.is_active && (
                    <button
                      onClick={() => handleSwitchAccount(account.id)}
                      className="bg-green-600 hover:bg-green-700 text-white text-sm font-medium py-1 px-3 rounded transition-colors"
                    >
                      {t('accounts.activate')}
                    </button>
                  )}
                  <button
                    onClick={() => handleDeleteAccount(account.id)}
                    className="bg-red-600 hover:bg-red-700 text-white text-sm font-medium py-1 px-3 rounded transition-colors"
                  >
                    {t('accounts.delete')}
                  </button>
                </div>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
