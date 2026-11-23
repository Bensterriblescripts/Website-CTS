(() => {
  const backToTop = document.getElementById('back-to-top');
  let pending = false;
  const updateBackToTop = () => {
    backToTop.classList.toggle('hide', scrollY <= 500);
    pending = false;
  };
  addEventListener('scroll', () => {
    if (pending) return;
    pending = true;
    requestAnimationFrame(updateBackToTop);
  }, { passive: true });
  updateBackToTop();

  const review = document.querySelector('.review-widget');
  if (document.cookie.split('; ').includes('REVIEW_WIDGET_CLOSED=1')) {
    review.remove();
  } else {
    review.querySelector('.review-widget-close').addEventListener('click', (event) => {
      event.preventDefault();
      document.cookie = 'REVIEW_WIDGET_CLOSED=1; max-age=31536000; path=/; SameSite=Lax';
      review.remove();
    });
  }

  const parts = Object.fromEntries(
    new Intl.DateTimeFormat('en-NZ', { day: 'numeric', month: 'long', year: 'numeric' })
      .formatToParts(new Date())
      .map((part) => [part.type, part.value]),
  );
  const day = Number(parts.day);
  const suffix = day % 100 >= 11 && day % 100 <= 13 ? 'th' : ['th', 'st', 'nd', 'rd'][day % 10] ?? 'th';
  for (const element of document.querySelectorAll('.last-updated')) {
    element.textContent = `${day}${suffix} ${parts.month}, ${parts.year}`;
  }
})();
