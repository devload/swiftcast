import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface UsageStats {
  total_requests: number;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cost_usd: number;
}

export default function UsageMonitor() {
  const [stats, setStats] = useState<UsageStats | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    loadStats();
    const interval = setInterval(loadStats, 5000); // 5초마다 갱신
    return () => clearInterval(interval);
  }, []);

  const loadStats = async () => {
    try {
      // TODO: 실제 통계 조회 구현
      // const result = await invoke('get_usage_statistics', {
      //   filter: {}
      // });
      // setStats(result as UsageStats);

      // 임시 데이터
      setStats({
        total_requests: 0,
        total_input_tokens: 0,
        total_output_tokens: 0,
        total_cost_usd: 0
      });
    } catch (error) {
      console.error('Failed to load stats:', error);
    } finally {
      setLoading(false);
    }
  };

  if (loading) {
    return (
      <div className="bg-white rounded-lg shadow p-6">
        <div className="text-center text-gray-500">로딩 중...</div>
      </div>
    );
  }

  return (
    <div className="bg-white rounded-lg shadow p-6">
      <h2 className="text-lg font-semibold text-gray-900 mb-4">사용량 모니터링</h2>

      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <div className="bg-gradient-to-br from-blue-50 to-blue-100 rounded-lg p-4">
          <div className="text-sm text-blue-600 font-medium mb-1">총 요청</div>
          <div className="text-2xl font-bold text-gray-900">{stats?.total_requests || 0}</div>
        </div>

        <div className="bg-gradient-to-br from-green-50 to-green-100 rounded-lg p-4">
          <div className="text-sm text-green-600 font-medium mb-1">입력 토큰</div>
          <div className="text-2xl font-bold text-gray-900">
            {(stats?.total_input_tokens || 0).toLocaleString()}
          </div>
        </div>

        <div className="bg-gradient-to-br from-purple-50 to-purple-100 rounded-lg p-4">
          <div className="text-sm text-purple-600 font-medium mb-1">출력 토큰</div>
          <div className="text-2xl font-bold text-gray-900">
            {(stats?.total_output_tokens || 0).toLocaleString()}
          </div>
        </div>

        <div className="bg-gradient-to-br from-orange-50 to-orange-100 rounded-lg p-4">
          <div className="text-sm text-orange-600 font-medium mb-1">총 비용</div>
          <div className="text-2xl font-bold text-gray-900">
            ${(stats?.total_cost_usd || 0).toFixed(4)}
          </div>
        </div>
      </div>

      <div className="mt-4 bg-gray-50 rounded-lg p-4">
        <div className="text-sm text-gray-600 text-center">
          💡 프록시를 통해 요청을 보내면 여기에 실시간으로 통계가 표시됩니다
        </div>
      </div>
    </div>
  );
}
