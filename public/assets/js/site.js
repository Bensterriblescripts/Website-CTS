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
})();
