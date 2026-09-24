function outer(a,
               b) {
  if (a &&
      b) {
    return a +
      b;
  }
  const value = compute(one,
                        two);
  items.forEach(item => {
    use(item);
  });
  promise.then(function (result) {
    return result;
  }).catch(err =>
    handle(err));
  const list = [
    1,
    2,
  ];
  const map = {
    key: value,
    other: [
      nested,
    ],
  };
  var a = 1,
      b = 2;
  if (x) y();
  else z();
  if (x)
    y();
  else if (w)
    z();
  else
    v();
  while (cond)
    step();
  switch (value) {
    case 1:
      one();
    default:
      other();
  }
  chained
    .method()
    .another();
  return {
    done: true
  };
}
