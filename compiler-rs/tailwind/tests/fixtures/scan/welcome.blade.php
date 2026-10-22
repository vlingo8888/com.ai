<x-app-layout>
    <div class="py-12 {{ $isActive ? 'bg-indigo-600 text-white' : 'bg-white' }}">
        <div @class(['p-4', 'font-bold' => $active, 'text-gray-400' => ! $active])>
            {{ __('Dashboard') }}
        </div>
        @if ($user->isAdmin())
            <span class="inline-flex items-center rounded-full bg-green-100 px-2.5 py-0.5 text-xs">Admin</span>
        @endif
        <?php echo '<p class="mt-4 text-sm">' . e($text) . '</p>'; ?>
        <div class="w-[calc(100%-1rem)] lg:max-w-7xl"></div>
    </div>
</x-app-layout>
