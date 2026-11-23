(() => {
  const apply = () => {
    const width = document.documentElement.clientWidth, cl = document.documentElement.classList;
    const device = width >= 1200 ? 'device-desktop' : width >= 577 ? 'device-tablet' : 'device-phone';
    cl.remove('device-desktop', 'device-tablet', 'device-phone');
    cl.add(device);
  };
  apply();
  addEventListener('resize', apply);
})();
