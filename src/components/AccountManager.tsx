import { useState, useEffect } from 'react';
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
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [showAddForm, setShowAddForm] = useState(false);
  const [newAccount, setNewAccount] = useState({
    name: '',
    base_url: '',
    api_key: ''
  });

  useEffect(() => {
    loadAccounts();
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
      alert('모든 필드를 입력해주세요');
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
      alert(`계정 생성 실패: ${error}`);
    }
  };

  const handleSwitchAccount = async (accountId: string) => {
    try {
      await invoke('switch_account', { accountId });
      await loadAccounts();
      onAccountChange();
    } catch (error) {
      console.error('Failed to switch account:', error);
      alert(`계정 전환 실패: ${error}`);
    }
  };

  const handleDeleteAccount = async (accountId: string) => {
    if (!confirm('정말 이 계정을 삭제하시겠습니까?')) {
      return;
    }

    try {
      await invoke('delete_account', { accountId });
      await loadAccounts();
      onAccountChange();
    } catch (error) {
      console.error('Failed to delete account:', error);
      alert(`계정 삭제 실패: ${error}`);
    }
  };

  return (
    <div className="bg-white rounded-lg shadow p-6">
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-lg font-semibold text-gray-900">계정 관리</h2>
        <button
          onClick={() => setShowAddForm(!showAddForm)}
          className="bg-blue-600 hover:bg-blue-700 text-white text-sm font-medium py-2 px-4 rounded-lg transition-colors"
        >
          {showAddForm ? '취소' : '+ 계정 추가'}
        </button>
      </div>

      {/* 계정 추가 폼 */}
      {showAddForm && (
        <div className="bg-gray-50 rounded-lg p-4 mb-4">
          <div className="space-y-3">
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">
                계정 이름
              </label>
              <input
                type="text"
                value={newAccount.name}
                onChange={(e) => setNewAccount({ ...newAccount, name: e.target.value })}
                placeholder="예: My GLM Account"
                className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">
                Base URL
              </label>
              <select
                value={newAccount.base_url}
                onChange={(e) => setNewAccount({ ...newAccount, base_url: e.target.value })}
                className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
              >
                <option value="">선택하세요</option>
                <option value="https://api.anthropic.com">Anthropic (Claude)</option>
                <option value="https://api.z.ai/api/anthropic">GLM (Z.AI)</option>
              </select>
            </div>
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">
                API Key
              </label>
              <input
                type="password"
                value={newAccount.api_key}
                onChange={(e) => setNewAccount({ ...newAccount, api_key: e.target.value })}
                placeholder="sk-ant-... 또는 GLM API 키"
                className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
              />
            </div>
            <button
              onClick={handleAddAccount}
              className="w-full bg-blue-600 hover:bg-blue-700 text-white font-medium py-2 px-4 rounded-lg transition-colors"
            >
              추가
            </button>
          </div>
        </div>
      )}

      {/* 계정 목록 */}
      <div className="space-y-2">
        {accounts.length === 0 ? (
          <div className="text-center text-gray-500 py-8">
            계정이 없습니다. 계정을 추가해주세요.
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
                        활성
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
                      활성화
                    </button>
                  )}
                  <button
                    onClick={() => handleDeleteAccount(account.id)}
                    className="bg-red-600 hover:bg-red-700 text-white text-sm font-medium py-1 px-3 rounded transition-colors"
                  >
                    삭제
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
