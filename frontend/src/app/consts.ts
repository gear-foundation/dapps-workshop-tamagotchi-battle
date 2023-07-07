import { HexString } from '@polkadot/util/types';

export const LOCAL_STORAGE = {
  ACCOUNT: 'account',
};

export const createTamagotchiInitial = {
  programId: '' as HexString,
  programId2: '' as HexString,
  currentStep: 1,
};

export const ROUTES = {
  HOME: '/',
  GAME: '/battle',
  TEST: '/test',
  NOTFOUND: '*'
}

export const ENV = {
  store: process.env.REACT_APP_STORE_ADDRESS as HexString,
  balance: process.env.REACT_APP_FT_ADDRESS as HexString,
  battle: process.env.REACT_APP_BATTLE_ADDRESS as HexString,
  NODE: process.env.REACT_APP_NODE_ADDRESS as string,
};

export const PLAYER_CARD = {
  spacing: {
    desktop: 8,
    mobile: 6,
  },
  width: {
    desktop: 160,
    mobile: 140,
  },
};
